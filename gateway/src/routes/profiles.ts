import type {
  DeletionRecord,
  Env,
  ListItemRecord,
  ListRecordRow,
  ProfileRecord,
  WatchStatusRecord,
} from '../types';
import { authenticateDevice, checkRateLimit } from '../auth';

const VALID_STATUSES = new Set(['to_watch', 'in_progress', 'watched']);

const KIND_PROFILE = 'profile';
const KIND_LIST = 'list';
const KIND_LIST_ITEM = 'list_item';
const KIND_WATCH_STATUS = 'watch_status';

const DELETION_RETENTION_MS = 90 * 24 * 60 * 60 * 1000;
const MAX_NAME_LENGTH = 100;
const MAX_ID_LENGTH = 200;
const MAX_TITLE_LENGTH = 300;
const MAX_URL_LENGTH = 2000;

// A profile can generate many small writes during a binge session (status
// updates, list edits), well above the shared per-IP budget in index.ts,
// which runs before device auth and is not meant to cap one device's normal
// use. This is a separate, per-device budget for the write endpoints only.
const WRITE_LIMIT_PER_MINUTE = 120;

// Every handler below scopes reads and writes to the caller's own invite -
// a device from one household never sees or touches another invite's
// profiles, lists, or watch status. There is no cross-invite listing.

async function enforceWriteBudget(env: Env, tokenHash: string): Promise<Response | null> {
  const allowed = await checkRateLimit(env, `profile-write:${tokenHash}`, WRITE_LIMIT_PER_MINUTE, 60);
  if (!allowed) {
    return Response.json({ error: 'Rate limit exceeded' }, { status: 429 });
  }
  return null;
}

async function parseJsonBody(request: Request): Promise<any | null> {
  try {
    return await request.json();
  } catch {
    return null;
  }
}

// ---- Input cleaning ----

function cleanText(value: unknown, max: number): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed && trimmed.length <= max ? trimmed : null;
}

function cleanId(value: unknown): string | null {
  return cleanText(value, MAX_ID_LENGTH);
}

// A poster must be a web URL. Anything else is dropped, not stored.
function cleanPoster(value: unknown): string | null {
  const url = cleanText(value, MAX_URL_LENGTH);
  return url && /^https?:\/\//i.test(url) ? url : null;
}

function cleanYear(value: unknown): number | null {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1800 && value <= 2200 ? value : null;
}

interface TitleFields {
  mediaType: string | null;
  title: string | null;
  poster: string | null;
  year: number | null;
  canonicalId: string | null;
}

function readTitleFields(body: any): TitleFields {
  return {
    mediaType: cleanText(body?.media_type, MAX_ID_LENGTH),
    title: cleanText(body?.title, MAX_TITLE_LENGTH),
    poster: cleanPoster(body?.poster),
    year: cleanYear(body?.year),
    canonicalId: cleanId(body?.canonical_id),
  };
}

function isUniqueViolation(error: unknown): boolean {
  return error instanceof Error && /UNIQUE constraint failed/i.test(error.message);
}

// ---- Titles, deletions, and alias ids ----

// A `null` field never overwrites a stored value, so a later call that only
// knows the catalog id cannot blank the title another call stored.
async function upsertTitle(
  env: Env,
  invite: string,
  catalogId: string,
  fields: TitleFields,
  now: number
): Promise<void> {
  if (!fields.mediaType && !fields.title && !fields.poster && fields.year === null && !fields.canonicalId) {
    return;
  }
  await env.DB.prepare(
    `INSERT INTO titles (invite_code_hash, catalog_id, canonical_id, media_type, title, poster, year, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(invite_code_hash, catalog_id) DO UPDATE SET
       canonical_id = COALESCE(excluded.canonical_id, titles.canonical_id),
       media_type = COALESCE(excluded.media_type, titles.media_type),
       title = COALESCE(excluded.title, titles.title),
       poster = COALESCE(excluded.poster, titles.poster),
       year = COALESCE(excluded.year, titles.year),
       updated_at = excluded.updated_at`
  )
    .bind(invite, catalogId, fields.canonicalId, fields.mediaType, fields.title, fields.poster, fields.year, now)
    .run();
}

async function recordDeletion(env: Env, invite: string, kind: string, key: string, at: number): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO deletions (invite_code_hash, kind, key, deleted_at) VALUES (?, ?, ?, ?)
     ON CONFLICT(invite_code_hash, kind, key) DO UPDATE SET deleted_at = excluded.deleted_at`
  )
    .bind(invite, kind, key, at)
    .run();
}

async function clearDeletion(env: Env, invite: string, kind: string, key: string): Promise<void> {
  await env.DB.prepare('DELETE FROM deletions WHERE invite_code_hash = ? AND kind = ? AND key = ?')
    .bind(invite, kind, key)
    .run();
}

// The id that names the same title across catalogs: the stored canonical id
// (for example the IMDb id of a `kitsu:` anime), or the catalog id itself.
async function canonicalOf(env: Env, invite: string, catalogId: string): Promise<string> {
  const row = await env.DB.prepare(
    'SELECT COALESCE(canonical_id, catalog_id) AS canonical FROM titles WHERE invite_code_hash = ? AND catalog_id = ?'
  )
    .bind(invite, catalogId)
    .first<{ canonical: string }>();
  return row?.canonical ?? catalogId;
}

// Another item of the same list that names the same title under a different id.
async function findAliasItem(env: Env, invite: string, listId: string, catalogId: string): Promise<string | null> {
  const canonical = await canonicalOf(env, invite, catalogId);
  const row = await env.DB.prepare(
    `SELECT list_items.catalog_id AS catalog_id
     FROM list_items
     LEFT JOIN titles ON titles.invite_code_hash = ? AND titles.catalog_id = list_items.catalog_id
     WHERE list_items.list_id = ? AND list_items.catalog_id != ?
       AND COALESCE(titles.canonical_id, list_items.catalog_id) = ?
     LIMIT 1`
  )
    .bind(invite, listId, catalogId, canonical)
    .first<{ catalog_id: string }>();
  return row?.catalog_id ?? null;
}

// Other statuses of the same profile that name the same title under a different id.
async function findAliasStatuses(
  env: Env,
  invite: string,
  profileId: string,
  catalogId: string
): Promise<{ catalog_id: string; updated_at: number }[]> {
  const canonical = await canonicalOf(env, invite, catalogId);
  const rows = await env.DB.prepare(
    `SELECT ws.catalog_id AS catalog_id, ws.updated_at AS updated_at
     FROM watch_status ws
     LEFT JOIN titles ON titles.invite_code_hash = ? AND titles.catalog_id = ws.catalog_id
     WHERE ws.profile_id = ? AND ws.catalog_id != ?
       AND COALESCE(titles.canonical_id, ws.catalog_id) = ?`
  )
    .bind(invite, profileId, catalogId, canonical)
    .all<{ catalog_id: string; updated_at: number }>();
  return rows.results || [];
}

// Confirms the profile exists and belongs to the caller's invite. Returns
// the profile row on success, or the error Response to return as-is.
async function loadOwnedProfile(
  env: Env,
  profileId: string,
  inviteCodeHash: string
): Promise<ProfileRecord | Response> {
  const profile = await env.DB.prepare(
    'SELECT id, invite_code_hash, name, avatar_key, created_at, updated_at FROM profiles WHERE id = ?'
  )
    .bind(profileId)
    .first<ProfileRecord>();
  if (!profile || profile.invite_code_hash !== inviteCodeHash) {
    return Response.json({ error: 'No profile with that id on this invite' }, { status: 404 });
  }
  return profile;
}

// Resolves a list's owning profile id and confirms it belongs to the
// caller's invite, so list and list-item routes never need the caller to
// separately pass a profile id.
async function loadOwnedList(
  env: Env,
  listId: string,
  inviteCodeHash: string
): Promise<ListRecordRow | Response> {
  const list = await env.DB.prepare('SELECT id, profile_id, name, created_at FROM lists WHERE id = ?')
    .bind(listId)
    .first<ListRecordRow>();
  if (!list) {
    return Response.json({ error: 'No list with that id' }, { status: 404 });
  }
  const owned = await loadOwnedProfile(env, list.profile_id, inviteCodeHash);
  if (owned instanceof Response) {
    return Response.json({ error: 'No list with that id on this invite' }, { status: 404 });
  }
  return list;
}

// ---- Profiles ----

export async function handleListProfiles(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rows = await env.DB.prepare(
    'SELECT id, invite_code_hash, name, avatar_key, created_at, updated_at FROM profiles WHERE invite_code_hash = ? ORDER BY created_at ASC'
  )
    .bind(authRes.device.invite_code_hash)
    .all<ProfileRecord>();

  return Response.json({ profiles: rows.results || [] });
}

// A client-supplied timestamp (see `id` below) must fall in a plausible
// range, or a device could force its write to always win a later
// last-write-wins merge by claiming a far-future `updated_at`.
const MIN_PLAUSIBLE_TIMESTAMP = Date.parse('2020-01-01T00:00:00Z');
const MAX_FUTURE_SKEW_MS = 5 * 60 * 1000;

function clampTimestamp(value: unknown, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return fallback;
  }
  const max = Date.now() + MAX_FUTURE_SKEW_MS;
  return Math.min(Math.max(value, MIN_PLAUSIBLE_TIMESTAMP), max);
}

export async function handleCreateProfile(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  if (!body) {
    return Response.json({ error: 'Invalid JSON payload' }, { status: 400 });
  }
  const name = typeof body.name === 'string' ? body.name.trim() : '';
  const avatarKey = typeof body.avatar_key === 'string' ? body.avatar_key.trim() : '';
  if (!name || !avatarKey) {
    return Response.json({ error: 'name and avatar_key are required' }, { status: 400 });
  }

  const now = Date.now();
  // The desktop app's own "create a profile" action never sends `id` - the
  // gateway mints one, exactly as before. Only the sync bridge (pushing a
  // profile that was first created on a local device) supplies `id`, so the
  // same profile carries the same id on both sides; without this, every
  // first sync would mint a second, unrelated id remotely and the two
  // copies would duplicate forever instead of ever being recognized as the
  // same profile again.
  const rawId = typeof body.id === 'string' ? body.id.trim() : '';
  if (rawId.length > 100) {
    return Response.json({ error: 'id is too long' }, { status: 400 });
  }
  const suppliedId = rawId;
  if (suppliedId) {
    // A random 128-bit id colliding by chance is practically impossible,
    // but this is an authenticated endpoint reachable by any device on any
    // invite - refuse outright rather than silently overwrite a different
    // invite's profile if a caller ever supplies someone else's id.
    const existing = await env.DB.prepare('SELECT invite_code_hash FROM profiles WHERE id = ?')
      .bind(suppliedId)
      .first<{ invite_code_hash: string }>();
    if (existing && existing.invite_code_hash !== authRes.device.invite_code_hash) {
      return Response.json({ error: 'That id belongs to a different invite' }, { status: 409 });
    }
  }
  const profile: ProfileRecord = {
    id: suppliedId || crypto.randomUUID(),
    invite_code_hash: authRes.device.invite_code_hash,
    name,
    avatar_key: avatarKey,
    created_at: clampTimestamp(body.created_at, now),
    updated_at: clampTimestamp(body.updated_at, now),
  };

  // ON CONFLICT makes a retried sync push idempotent: a plain create (fresh
  // random id) never collides, so the clause is inert on that path; a sync
  // push that already succeeded once but lost its response just re-applies
  // the same values instead of failing on the id it already wrote.
  const result = await env.DB.prepare(
    `INSERT INTO profiles (id, invite_code_hash, name, avatar_key, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       name = excluded.name,
       avatar_key = excluded.avatar_key,
       updated_at = excluded.updated_at`
  )
    .bind(profile.id, profile.invite_code_hash, profile.name, profile.avatar_key, profile.created_at, profile.updated_at)
    .run();

  if (!result.success) {
    return Response.json({ error: 'Failed to create profile' }, { status: 500 });
  }
  await clearDeletion(env, profile.invite_code_hash, KIND_PROFILE, profile.id);
  return Response.json(profile);
}

export async function handleUpdateProfile(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const profileId = typeof body?.profile_id === 'string' ? body.profile_id : '';
  if (!profileId) {
    return Response.json({ error: 'profile_id is required' }, { status: 400 });
  }
  const owned = await loadOwnedProfile(env, profileId, authRes.device.invite_code_hash);
  if (owned instanceof Response) return owned;

  const name = typeof body.name === 'string' && body.name.trim() ? body.name.trim() : owned.name;
  const avatarKey =
    typeof body.avatar_key === 'string' && body.avatar_key.trim() ? body.avatar_key.trim() : owned.avatar_key;

  const result = await env.DB.prepare('UPDATE profiles SET name = ?, avatar_key = ?, updated_at = ? WHERE id = ?')
    .bind(name, avatarKey, Date.now(), profileId)
    .run();

  if (!result.success) {
    return Response.json({ error: 'Failed to update profile' }, { status: 500 });
  }
  return Response.json({ id: profileId, name, avatar_key: avatarKey });
}

// Removes a profile along with its lists, list items, and watch status. D1
// does not enforce ON DELETE CASCADE reliably across requests, so every
// dependent table is cleared explicitly, children first.
export async function handleDeleteProfile(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const profileId = typeof body?.profile_id === 'string' ? body.profile_id : '';
  if (!profileId) {
    return Response.json({ error: 'profile_id is required' }, { status: 400 });
  }
  const owned = await loadOwnedProfile(env, profileId, authRes.device.invite_code_hash);
  if (owned instanceof Response) return owned;

  await env.DB.prepare('DELETE FROM watch_status WHERE profile_id = ?').bind(profileId).run();
  await env.DB.prepare(
    'DELETE FROM list_items WHERE list_id IN (SELECT id FROM lists WHERE profile_id = ?)'
  )
    .bind(profileId)
    .run();
  await env.DB.prepare('DELETE FROM lists WHERE profile_id = ?').bind(profileId).run();
  const result = await env.DB.prepare('DELETE FROM profiles WHERE id = ?').bind(profileId).run();

  if (!result.success) {
    return Response.json({ error: 'Failed to delete profile' }, { status: 500 });
  }
  await recordDeletion(env, authRes.device.invite_code_hash, KIND_PROFILE, profileId, Date.now());
  return Response.json({ deleted: true });
}

// ---- Lists ----

export async function handleListLists(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const url = new URL(request.url);
  const profileId = url.searchParams.get('profile_id') || '';
  if (!profileId) {
    return Response.json({ error: 'profile_id is required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedProfile(env, profileId, invite);
  if (owned instanceof Response) return owned;

  const rows = await env.DB.prepare(
    `SELECT lists.id AS list_id, lists.name AS list_name, lists.created_at AS list_created_at,
            list_items.catalog_id AS catalog_id, list_items.media_type AS media_type, list_items.added_at AS added_at,
            titles.title AS title, titles.poster AS poster, titles.year AS year, titles.canonical_id AS canonical_id
     FROM lists
     LEFT JOIN list_items ON list_items.list_id = lists.id
     LEFT JOIN titles ON titles.invite_code_hash = ? AND titles.catalog_id = list_items.catalog_id
     WHERE lists.profile_id = ?
     ORDER BY lists.created_at ASC, list_items.added_at ASC`
  )
    .bind(invite, profileId)
    .all<{
      list_id: string;
      list_name: string;
      list_created_at: number;
      catalog_id: string | null;
      media_type: string | null;
      added_at: number | null;
      title: string | null;
      poster: string | null;
      year: number | null;
      canonical_id: string | null;
    }>();

  const lists = new Map<string, { id: string; name: string; created_at: number; items: ListItemRecord[] }>();
  for (const row of rows.results || []) {
    if (!lists.has(row.list_id)) {
      lists.set(row.list_id, { id: row.list_id, name: row.list_name, created_at: row.list_created_at, items: [] });
    }
    if (row.catalog_id && row.media_type && row.added_at !== null) {
      lists.get(row.list_id)!.items.push({
        list_id: row.list_id,
        catalog_id: row.catalog_id,
        media_type: row.media_type,
        added_at: row.added_at,
        title: row.title,
        poster: row.poster,
        year: row.year,
        canonical_id: row.canonical_id,
      });
    }
  }

  return Response.json({ lists: Array.from(lists.values()) });
}

export async function handleCreateList(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const profileId = typeof body?.profile_id === 'string' ? body.profile_id : '';
  const name = typeof body?.name === 'string' ? body.name.trim() : '';
  if (!profileId || !name) {
    return Response.json({ error: 'profile_id and name are required' }, { status: 400 });
  }
  if (name.length > MAX_NAME_LENGTH) {
    return Response.json({ error: 'name is too long' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedProfile(env, profileId, invite);
  if (owned instanceof Response) return owned;

  const suppliedId = typeof body?.id === 'string' ? body.id.trim() : '';
  if (suppliedId) {
    const existing = await env.DB.prepare(
      `SELECT profiles.invite_code_hash
       FROM lists
       JOIN profiles ON profiles.id = lists.profile_id
       WHERE lists.id = ?`
    )
      .bind(suppliedId)
      .first<{ invite_code_hash: string }>();
    if (existing && existing.invite_code_hash !== invite) {
      return Response.json({ error: 'That id belongs to a different invite' }, { status: 409 });
    }
  }

  const now = Date.now();
  const list: ListRecordRow = {
    id: suppliedId || crypto.randomUUID(),
    profile_id: profileId,
    name,
    created_at: clampTimestamp(body?.created_at, now),
  };
  try {
    await env.DB.prepare(
      `INSERT INTO lists (id, profile_id, name, created_at)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(id) DO UPDATE SET
         name = excluded.name`
    )
      .bind(list.id, list.profile_id, list.name, list.created_at)
      .run();
  } catch (error) {
    if (isUniqueViolation(error)) {
      return Response.json({ error: 'A list with that name already exists' }, { status: 409 });
    }
    return Response.json({ error: 'Failed to create list' }, { status: 500 });
  }
  await clearDeletion(env, invite, KIND_LIST, list.id);
  return Response.json(list);
}

export async function handleDeleteList(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const listId = typeof body?.list_id === 'string' ? body.list_id : '';
  if (!listId) {
    return Response.json({ error: 'list_id is required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedList(env, listId, invite);
  if (owned instanceof Response) return owned;

  await env.DB.prepare('DELETE FROM list_items WHERE list_id = ?').bind(listId).run();
  const result = await env.DB.prepare('DELETE FROM lists WHERE id = ?').bind(listId).run();

  if (!result.success) {
    return Response.json({ error: 'Failed to delete list' }, { status: 500 });
  }
  await recordDeletion(env, invite, KIND_LIST, listId, Date.now());
  return Response.json({ deleted: true });
}

export async function handleAddListItem(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const listId = typeof body?.list_id === 'string' ? body.list_id : '';
  const catalogId = cleanId(body?.catalog_id);
  const mediaType = cleanText(body?.media_type, MAX_ID_LENGTH);
  if (!listId || !catalogId || !mediaType) {
    return Response.json({ error: 'list_id, catalog_id, and media_type are required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedList(env, listId, invite);
  if (owned instanceof Response) return owned;

  const now = Date.now();
  await upsertTitle(env, invite, catalogId, { ...readTitleFields(body), mediaType }, now);
  const alias = await findAliasItem(env, invite, listId, catalogId);
  if (alias) {
    return Response.json({ added: false, alias_of: alias });
  }

  await env.DB.prepare(
    `INSERT INTO list_items (list_id, catalog_id, media_type, added_at)
     VALUES (?, ?, ?, ?)
     ON CONFLICT(list_id, catalog_id) DO UPDATE SET
       media_type = excluded.media_type,
       added_at = excluded.added_at`
  )
    .bind(listId, catalogId, mediaType, clampTimestamp(body?.added_at, now))
    .run();
  await clearDeletion(env, invite, KIND_LIST_ITEM, `${listId}:${catalogId}`);
  return Response.json({ added: true });
}

export async function handleRemoveListItem(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const listId = typeof body?.list_id === 'string' ? body.list_id : '';
  const catalogId = typeof body?.catalog_id === 'string' ? body.catalog_id : '';
  if (!listId || !catalogId) {
    return Response.json({ error: 'list_id and catalog_id are required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedList(env, listId, invite);
  if (owned instanceof Response) return owned;

  const result = await env.DB.prepare('DELETE FROM list_items WHERE list_id = ? AND catalog_id = ?')
    .bind(listId, catalogId)
    .run();

  if (!result.success) {
    return Response.json({ error: 'Failed to remove list item' }, { status: 500 });
  }
  await recordDeletion(env, invite, KIND_LIST_ITEM, `${listId}:${catalogId}`, Date.now());
  return Response.json({ removed: true });
}

// ---- Watch status ----

export async function handleListWatchStatus(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const url = new URL(request.url);
  const profileId = url.searchParams.get('profile_id') || '';
  if (!profileId) {
    return Response.json({ error: 'profile_id is required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedProfile(env, profileId, invite);
  if (owned instanceof Response) return owned;

  const status = url.searchParams.get('status');
  if (status && !VALID_STATUSES.has(status)) {
    return Response.json({ error: 'status must be to_watch, in_progress, or watched' }, { status: 400 });
  }

  const select = `SELECT ws.profile_id, ws.catalog_id, ws.status, ws.episode_id, ws.resume_seconds, ws.updated_at,
            titles.title AS title, titles.poster AS poster, titles.media_type AS media_type,
            titles.year AS year, titles.canonical_id AS canonical_id
     FROM watch_status ws
     LEFT JOIN titles ON titles.invite_code_hash = ? AND titles.catalog_id = ws.catalog_id
     WHERE ws.profile_id = ?`;
  const stmt = status
    ? env.DB.prepare(`${select} AND ws.status = ? ORDER BY ws.updated_at DESC`).bind(invite, profileId, status)
    : env.DB.prepare(`${select} ORDER BY ws.updated_at DESC`).bind(invite, profileId);
  const rows = await stmt.all<WatchStatusRecord>();

  return Response.json({ watch_status: rows.results || [] });
}

export async function handleSetWatchStatus(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const profileId = typeof body?.profile_id === 'string' ? body.profile_id : '';
  const catalogId = cleanId(body?.catalog_id);
  const status = typeof body?.status === 'string' ? body.status : '';
  if (!profileId || !catalogId || !VALID_STATUSES.has(status)) {
    return Response.json(
      { error: 'profile_id, catalog_id, and a valid status (to_watch, in_progress, watched) are required' },
      { status: 400 }
    );
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedProfile(env, profileId, invite);
  if (owned instanceof Response) return owned;

  const now = Date.now();
  const updatedAt = clampTimestamp(body.updated_at, now);
  await upsertTitle(env, invite, catalogId, readTitleFields(body), now);

  const aliases = await findAliasStatuses(env, invite, profileId, catalogId);
  // A newer status under an alias id already covers this title.
  if (aliases.some((alias) => alias.updated_at > updatedAt)) {
    return Response.json({ profile_id: profileId, catalog_id: catalogId, status, skipped: true });
  }

  const episodeId = typeof body.episode_id === 'string' ? body.episode_id : null;
  const resumeSeconds = typeof body.resume_seconds === 'number' ? body.resume_seconds : null;
  await env.DB.prepare(
    `INSERT INTO watch_status (profile_id, catalog_id, status, episode_id, resume_seconds, updated_at)
     VALUES (?, ?, ?, ?, ?, ?)
     ON CONFLICT(profile_id, catalog_id) DO UPDATE SET
       status = excluded.status,
       episode_id = excluded.episode_id,
       resume_seconds = excluded.resume_seconds,
       updated_at = excluded.updated_at`
  )
    .bind(profileId, catalogId, status, episodeId, resumeSeconds, updatedAt)
    .run();
  await clearDeletion(env, invite, KIND_WATCH_STATUS, `${profileId}:${catalogId}`);

  for (const alias of aliases) {
    await env.DB.prepare('DELETE FROM watch_status WHERE profile_id = ? AND catalog_id = ?')
      .bind(profileId, alias.catalog_id)
      .run();
    await recordDeletion(env, invite, KIND_WATCH_STATUS, `${profileId}:${alias.catalog_id}`, now);
  }
  return Response.json({ profile_id: profileId, catalog_id: catalogId, status });
}

export async function handleRemoveWatchStatus(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const budgetErr = await enforceWriteBudget(env, authRes.tokenHash);
  if (budgetErr) return budgetErr;

  const body = await parseJsonBody(request);
  const profileId = typeof body?.profile_id === 'string' ? body.profile_id : '';
  const catalogId = typeof body?.catalog_id === 'string' ? body.catalog_id : '';
  if (!profileId || !catalogId) {
    return Response.json({ error: 'profile_id and catalog_id are required' }, { status: 400 });
  }
  const invite = authRes.device.invite_code_hash;
  const owned = await loadOwnedProfile(env, profileId, invite);
  if (owned instanceof Response) return owned;

  const result = await env.DB.prepare('DELETE FROM watch_status WHERE profile_id = ? AND catalog_id = ?')
    .bind(profileId, catalogId)
    .run();
  if (!result.success) {
    return Response.json({ error: 'Failed to remove watch status' }, { status: 500 });
  }
  await recordDeletion(env, invite, KIND_WATCH_STATUS, `${profileId}:${catalogId}`, Date.now());
  return Response.json({ removed: true });
}

// ---- Deletions ----

// Returns every deletion record of the caller's invite, so a device can tell
// "removed elsewhere" from "never existed here" during sync.
export async function handleListDeletions(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;
  const invite = authRes.device.invite_code_hash;

  await env.DB.prepare('DELETE FROM deletions WHERE invite_code_hash = ? AND deleted_at < ?')
    .bind(invite, Date.now() - DELETION_RETENTION_MS)
    .run();
  const rows = await env.DB.prepare('SELECT kind, key, deleted_at FROM deletions WHERE invite_code_hash = ?')
    .bind(invite)
    .all<DeletionRecord>();
  return Response.json({ deletions: rows.results || [] });
}
