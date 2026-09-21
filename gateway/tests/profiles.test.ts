import { describe, it, expect } from 'vitest';
import { createRequire } from 'node:module';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// node:sqlite is newer than the bundler's built-in module list, so it is loaded
// at run time instead of through a static import, matching auth_and_invites.test.ts.
const { DatabaseSync } = createRequire(import.meta.url)('node:sqlite');
import { sha256Hex, generateSecureToken } from '../src/crypto';
import {
  handleListProfiles,
  handleCreateProfile,
  handleUpdateProfile,
  handleDeleteProfile,
  handleListLists,
  handleCreateList,
  handleDeleteList,
  handleAddListItem,
  handleRemoveListItem,
  handleListWatchStatus,
  handleSetWatchStatus,
} from '../src/routes/profiles';
import type { Env } from '../src/types';
import worker from '../src/index';

// The env applies the real migration files in order, so the tests cannot
// drift from the schema that production runs. It has every table the profiles
// routes touch, including devices and invites, so authenticateDevice and the
// ownership checks run against a real database, not mocks.
function fullSqliteEnv(): Env {
  const db = new DatabaseSync(':memory:');
  const dir = fileURLToPath(new URL('../migrations/', import.meta.url).href);
  for (const file of readdirSync(dir).filter((name) => name.endsWith('.sql')).sort()) {
    db.exec(readFileSync(dir + file, 'utf8'));
  }

  const DB: any = {
    prepare(sql: string) {
      const statement = db.prepare(sql);
      return {
        bind(...values: unknown[]) {
          return {
            async first<T>(): Promise<T | null> {
              return (statement.get(...(values as any[])) as T) ?? null;
            },
            async run() {
              const info = statement.run(...(values as any[]));
              return { success: true, meta: { changes: Number(info.changes) } };
            },
            async all<T>(): Promise<{ results: T[] }> {
              return { results: statement.all(...(values as any[])) as T[] };
            },
          };
        },
      };
    },
  };

  return { DB, OPENSUBTITLES_COORDINATOR: {} as any };
}

// Registers a device on a fresh invite and returns its raw bearer token plus
// the invite's code hash, so tests can authenticate as that device.
async function registerDevice(env: Env, rawInviteCode: string): Promise<{ token: string; inviteCodeHash: string }> {
  const codeHash = await sha256Hex(rawInviteCode);
  await env.DB.prepare(
    'INSERT INTO invites (code_hash, created_at, max_devices, redeemed_count, is_revoked, label) VALUES (?, ?, ?, ?, ?, ?)'
  )
    .bind(codeHash, Date.now(), 5, 0, 0, null)
    .run();

  const rawToken = generateSecureToken(32);
  const tokenHash = await sha256Hex(rawToken);
  await env.DB.prepare(
    'INSERT INTO devices (token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count) VALUES (?, ?, ?, ?, 0, 0)'
  )
    .bind(tokenHash, codeHash, Date.now(), Date.now())
    .run();

  return { token: rawToken, inviteCodeHash: codeHash };
}

function authedRequest(url: string, token: string, options: RequestInit = {}): Request {
  const headers = new Headers(options.headers || {});
  headers.set('Authorization', `Bearer ${token}`);
  if (options.body) headers.set('Content-Type', 'application/json');
  return new Request(url, { ...options, headers });
}

describe('Gateway Profiles', () => {
  it('creates a profile and lists it back for the same device', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');

    const createRes = await handleCreateProfile(
      authedRequest('https://gateway/v1/profiles', token, {
        method: 'POST',
        body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
      }),
      env
    );
    expect(createRes.status).toBe(200);
    const created: any = await createRes.json();
    expect(created.name).toBe('Ada');

    const listRes = await handleListProfiles(authedRequest('https://gateway/v1/profiles', token), env);
    const listed: any = await listRes.json();
    expect(listed.profiles).toHaveLength(1);
    expect(listed.profiles[0].id).toBe(created.id);
  });

  it('never lists another invite\'s profiles', async () => {
    const env = fullSqliteEnv();
    const a = await registerDevice(env, 'invite-a');
    const b = await registerDevice(env, 'invite-b');

    await handleCreateProfile(
      authedRequest('https://gateway/v1/profiles', a.token, {
        method: 'POST',
        body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
      }),
      env
    );

    const listRes = await handleListProfiles(authedRequest('https://gateway/v1/profiles', b.token), env);
    const listed: any = await listRes.json();
    expect(listed.profiles).toHaveLength(0);
  });

  it('rejects creating a profile with a missing name or avatar_key', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const res = await handleCreateProfile(
      authedRequest('https://gateway/v1/profiles', token, { method: 'POST', body: JSON.stringify({ name: '' }) }),
      env
    );
    expect(res.status).toBe(400);
  });

  it('updates a profile name and avatar', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const created: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    const updateRes = await handleUpdateProfile(
      authedRequest('https://gateway/v1/profiles/update', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: created.id, name: 'Adaeze' }),
      }),
      env
    );
    expect(updateRes.status).toBe(200);
    const updated: any = await updateRes.json();
    expect(updated.name).toBe('Adaeze');
    expect(updated.avatar_key).toBe('star-violet');
  });

  it('refuses to update a profile that belongs to a different invite', async () => {
    const env = fullSqliteEnv();
    const a = await registerDevice(env, 'invite-a');
    const b = await registerDevice(env, 'invite-b');
    const created: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', a.token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    const res = await handleUpdateProfile(
      authedRequest('https://gateway/v1/profiles/update', b.token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: created.id, name: 'Hijacked' }),
      }),
      env
    );
    expect(res.status).toBe(404);
  });

  it('deleting a profile cascades to its lists, items, and watch status', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    const list: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Weekend picks' }),
        }),
        env
      )
    ).json();
    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123', media_type: 'movie' }),
      }),
      env
    );
    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, catalog_id: 'tt123', status: 'in_progress' }),
      }),
      env
    );

    const deleteRes = await handleDeleteProfile(
      authedRequest('https://gateway/v1/profiles/delete', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id }),
      }),
      env
    );
    expect(deleteRes.status).toBe(200);

    const profiles: any = await (await handleListProfiles(authedRequest('https://gateway/v1/profiles', token), env)).json();
    expect(profiles.profiles).toHaveLength(0);

    const remainingLists = await env.DB.prepare('SELECT * FROM lists WHERE profile_id = ?').bind(profile.id).all();
    expect(remainingLists.results).toHaveLength(0);
    const remainingItems = await env.DB.prepare('SELECT * FROM list_items WHERE list_id = ?').bind(list.id).all();
    expect(remainingItems.results).toHaveLength(0);
    const remainingStatus = await env.DB.prepare('SELECT * FROM watch_status WHERE profile_id = ?')
      .bind(profile.id)
      .all();
    expect(remainingStatus.results).toHaveLength(0);
  });

  it('creates a list, adds items, and lists them back grouped under the list', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    const list: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Weekend picks' }),
        }),
        env
      )
    ).json();

    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123', media_type: 'movie' }),
      }),
      env
    );
    // Adding the same title twice must not duplicate the row.
    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123', media_type: 'movie' }),
      }),
      env
    );
    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt456', media_type: 'series' }),
      }),
      env
    );

    const listsRes = await handleListLists(
      authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token),
      env
    );
    const lists: any = await listsRes.json();
    expect(lists.lists).toHaveLength(1);
    expect(lists.lists[0].items).toHaveLength(2);

    await handleRemoveListItem(
      authedRequest('https://gateway/v1/profiles/lists/items/delete', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123' }),
      }),
      env
    );

    const afterRemove: any = await (
      await handleListLists(authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token), env)
    ).json();
    expect(afterRemove.lists[0].items).toHaveLength(1);
    expect(afterRemove.lists[0].items[0].catalog_id).toBe('tt456');
  });

  it('stores and returns a list item\'s title and poster', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    const list: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Weekend picks' }),
        }),
        env
      )
    ).json();

    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({
          list_id: list.id,
          catalog_id: 'tt123',
          media_type: 'movie',
          title: 'Shameless',
          poster: 'https://example.com/poster.jpg',
        }),
      }),
      env
    );

    const listsRes: any = await (
      await handleListLists(authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token), env)
    ).json();
    expect(listsRes.lists[0].items[0].title).toBe('Shameless');
    expect(listsRes.lists[0].items[0].poster).toBe('https://example.com/poster.jpg');
  });

  it('a re-push with no title keeps the title a previous push already set', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    const list: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Weekend picks' }),
        }),
        env
      )
    ).json();

    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123', media_type: 'movie', title: 'Shameless' }),
      }),
      env
    );
    // A later push (e.g. a retried sync) with no title must not blank it out.
    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: list.id, catalog_id: 'tt123', media_type: 'movie' }),
      }),
      env
    );

    const listsRes: any = await (
      await handleListLists(authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token), env)
    ).json();
    expect(listsRes.lists[0].items[0].title).toBe('Shameless');
  });

  it('an empty list still appears with zero items', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    await handleCreateList(
      authedRequest('https://gateway/v1/profiles/lists', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, name: 'Empty for now' }),
      }),
      env
    );

    const lists: any = await (
      await handleListLists(authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token), env)
    ).json();
    expect(lists.lists).toHaveLength(1);
    expect(lists.lists[0].items).toHaveLength(0);
  });

  it('deleting a list removes only its own items', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();
    const keep: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Weekend picks' }),
        }),
        env
      )
    ).json();
    const discard: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({ profile_id: profile.id, name: 'Anime to try' }),
        }),
        env
      )
    ).json();
    await handleAddListItem(
      authedRequest('https://gateway/v1/profiles/lists/items', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: discard.id, catalog_id: 'tt123', media_type: 'movie' }),
      }),
      env
    );

    await handleDeleteList(
      authedRequest('https://gateway/v1/profiles/lists/delete', token, {
        method: 'POST',
        body: JSON.stringify({ list_id: discard.id }),
      }),
      env
    );

    const lists: any = await (
      await handleListLists(authedRequest(`https://gateway/v1/profiles/lists?profile_id=${profile.id}`, token), env)
    ).json();
    expect(lists.lists).toHaveLength(1);
    expect(lists.lists[0].id).toBe(keep.id);
  });

  it('stores a title/poster/media_type on watch status and keeps them on a later update with none', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({
          profile_id: profile.id,
          catalog_id: 'tt1',
          status: 'to_watch',
          title: 'Shameless',
          poster: 'https://example.com/poster.jpg',
          media_type: 'series',
        }),
      }),
      env
    );
    // The progress tracker's own periodic push has none of these - must not wipe them.
    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, catalog_id: 'tt1', status: 'in_progress', resume_seconds: 600 }),
      }),
      env
    );

    const res: any = await (
      await handleListWatchStatus(authedRequest(`https://gateway/v1/profiles/status?profile_id=${profile.id}`, token), env)
    ).json();
    expect(res.watch_status[0].status).toBe('in_progress');
    expect(res.watch_status[0].resume_seconds).toBe(600);
    expect(res.watch_status[0].media_type).toBe('series');
    expect(res.watch_status[0].title).toBe('Shameless');
    expect(res.watch_status[0].poster).toBe('https://example.com/poster.jpg');
  });

  it('sets and updates watch status, grouped correctly by column', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, catalog_id: 'tt1', status: 'to_watch' }),
      }),
      env
    );
    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({
          profile_id: profile.id,
          catalog_id: 'tt2',
          status: 'in_progress',
          episode_id: 'tt2:1:2',
          resume_seconds: 930,
        }),
      }),
      env
    );

    const inProgress: any = await (
      await handleListWatchStatus(
        authedRequest(`https://gateway/v1/profiles/status?profile_id=${profile.id}&status=in_progress`, token),
        env
      )
    ).json();
    expect(inProgress.watch_status).toHaveLength(1);
    expect(inProgress.watch_status[0].catalog_id).toBe('tt2');
    expect(inProgress.watch_status[0].resume_seconds).toBe(930);

    // Re-setting the same title updates in place instead of inserting again.
    await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, catalog_id: 'tt2', status: 'watched' }),
      }),
      env
    );
    const all: any = await (
      await handleListWatchStatus(authedRequest(`https://gateway/v1/profiles/status?profile_id=${profile.id}`, token), env)
    ).json();
    expect(all.watch_status).toHaveLength(2);
    const stillInProgress: any = await (
      await handleListWatchStatus(
        authedRequest(`https://gateway/v1/profiles/status?profile_id=${profile.id}&status=in_progress`, token),
        env
      )
    ).json();
    expect(stillInProgress.watch_status).toHaveLength(0);
  });

  it('rejects an invalid status value', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    const res = await handleSetWatchStatus(
      authedRequest('https://gateway/v1/profiles/status', token, {
        method: 'POST',
        body: JSON.stringify({ profile_id: profile.id, catalog_id: 'tt1', status: 'binging' }),
      }),
      env
    );
    expect(res.status).toBe(400);
  });

  it('rejects every profiles route for a revoked device', async () => {
    const env = fullSqliteEnv();
    const { token, inviteCodeHash } = await registerDevice(env, 'invite-a');
    const tokenHash = await sha256Hex(token);
    await env.DB.prepare('UPDATE devices SET is_revoked = 1 WHERE token_hash = ?').bind(tokenHash).run();

    const res = await handleListProfiles(authedRequest('https://gateway/v1/profiles', token), env);
    expect(res.status).toBe(401);
    expect(inviteCodeHash).toBeDefined();
  });

  it('accepts a client-supplied id, for the sync bridge to keep the same profile id on both sides', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');

    const created: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({
            id: 'local-device-id-1',
            name: 'Ada',
            avatar_key: 'star-violet',
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
          }),
        }),
        env
      )
    ).json();

    expect(created.id).toBe('local-device-id-1');
    expect(created.created_at).toBe(1_700_000_000_000);
  });

  it('re-pushing the same id is idempotent instead of failing on a retry', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const push = () =>
      handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({
            id: 'local-device-id-1',
            name: 'Ada',
            avatar_key: 'star-violet',
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
          }),
        }),
        env
      );

    const first = await push();
    expect(first.status).toBe(200);
    const second = await push();
    expect(second.status).toBe(200);

    const rows: any = await env.DB.prepare('SELECT * FROM profiles WHERE id = ?')
      .bind('local-device-id-1')
      .all();
    expect(rows.results).toHaveLength(1);
  });

  it('refuses a supplied id that already belongs to a different invite', async () => {
    const env = fullSqliteEnv();
    const a = await registerDevice(env, 'invite-a');
    const b = await registerDevice(env, 'invite-b');

    await handleCreateProfile(
      authedRequest('https://gateway/v1/profiles', a.token, {
        method: 'POST',
        body: JSON.stringify({ id: 'shared-id', name: 'Ada', avatar_key: 'star-violet' }),
      }),
      env
    );

    const res = await handleCreateProfile(
      authedRequest('https://gateway/v1/profiles', b.token, {
        method: 'POST',
        body: JSON.stringify({ id: 'shared-id', name: 'Hijack', avatar_key: 'bolt-amber' }),
      }),
      env
    );
    expect(res.status).toBe(409);
  });

  it('clamps an implausible timestamp instead of trusting it outright', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');

    const created: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({
            name: 'Ada',
            avatar_key: 'star-violet',
            updated_at: 9_999_999_999_999, // far future
          }),
        }),
        env
      )
    ).json();

    expect(created.updated_at).toBeLessThan(9_999_999_999_999);
  });

  it('accepts a client-supplied id for lists and supports idempotent push', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const profile: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    const list1: any = await (
      await handleCreateList(
        authedRequest('https://gateway/v1/profiles/lists', token, {
          method: 'POST',
          body: JSON.stringify({
            id: 'list-uuid-1',
            profile_id: profile.id,
            name: 'Sci-Fi',
            created_at: 1_700_000_000_000,
          }),
        }),
        env
      )
    ).json();

    expect(list1.id).toBe('list-uuid-1');
    expect(list1.created_at).toBe(1_700_000_000_000);

    // Retrying with same ID is idempotent
    const res2 = await handleCreateList(
      authedRequest('https://gateway/v1/profiles/lists', token, {
        method: 'POST',
        body: JSON.stringify({
          id: 'list-uuid-1',
          profile_id: profile.id,
          name: 'Sci-Fi Favorites',
          created_at: 1_700_000_000_000,
        }),
      }),
      env
    );
    expect(res2.status).toBe(200);

    const rows: any = await env.DB.prepare('SELECT * FROM lists WHERE id = ?').bind('list-uuid-1').all();
    expect(rows.results).toHaveLength(1);
    expect(rows.results[0].name).toBe('Sci-Fi Favorites');
  });

  it('refuses a supplied list id that already belongs to a different invite', async () => {
    const env = fullSqliteEnv();
    const a = await registerDevice(env, 'invite-a');
    const b = await registerDevice(env, 'invite-b');

    const profileA: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', a.token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
        }),
        env
      )
    ).json();

    const profileB: any = await (
      await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', b.token, {
          method: 'POST',
          body: JSON.stringify({ name: 'Bob', avatar_key: 'bolt-amber' }),
        }),
        env
      )
    ).json();

    await handleCreateList(
      authedRequest('https://gateway/v1/profiles/lists', a.token, {
        method: 'POST',
        body: JSON.stringify({ id: 'shared-list-id', profile_id: profileA.id, name: 'List A' }),
      }),
      env
    );

    const res = await handleCreateList(
      authedRequest('https://gateway/v1/profiles/lists', b.token, {
        method: 'POST',
        body: JSON.stringify({ id: 'shared-list-id', profile_id: profileB.id, name: 'Hijack' }),
      }),
      env
    );
    expect(res.status).toBe(409);
  });

  it('routes a profile request through the full worker, headers included', async () => {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');
    const ctx = {} as ExecutionContext;

    const res = await worker.fetch(
      authedRequest('https://gateway/v1/profiles', token, {
        method: 'POST',
        body: JSON.stringify({ name: 'Ada', avatar_key: 'star-violet' }),
      }),
      env,
      ctx
    );
    expect(res.status).toBe(200);
    expect(res.headers.get('X-Content-Type-Options')).toBe('nosniff');
  });

  it('caps writes for one device at the per-device write budget', async () => {
    // Calls the handler directly, bypassing the router's shared per-IP budget
    // in index.ts (60/min, keyed by 'unknown' with no CF-Connecting-IP header
    // here) - it is smaller than the 120/min write budget and would trip
    // first through the full worker, masking the thing this test checks.
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, 'invite-a');

    const codes: number[] = [];
    for (let attempt = 0; attempt < 121; attempt += 1) {
      const res = await handleCreateProfile(
        authedRequest('https://gateway/v1/profiles', token, {
          method: 'POST',
          body: JSON.stringify({ name: `Profile ${attempt}`, avatar_key: 'star-violet' }),
        }),
        env
      );
      codes.push(res.status);
    }
    expect(codes.slice(0, 120).every((c) => c === 200)).toBe(true);
    expect(codes[120]).toBe(429);
  });
});

describe('Gateway profiles: titles, deletions, names, and alias ids', () => {
  const ctx = {} as ExecutionContext;

  async function call(env: Env, token: string, method: string, path: string, body?: unknown) {
    const res = await worker.fetch(
      authedRequest(`https://gateway${path}`, token, {
        method,
        body: body === undefined ? undefined : JSON.stringify(body),
      }),
      env,
      ctx
    );
    return { status: res.status, json: (await res.json()) as any };
  }

  async function setup(inviteCode = 'invite-a') {
    const env = fullSqliteEnv();
    const { token } = await registerDevice(env, inviteCode);
    const profile = (await call(env, token, 'POST', '/v1/profiles', { name: 'Ada', avatar_key: 'amber' })).json;
    const list = (await call(env, token, 'POST', '/v1/profiles/lists', { profile_id: profile.id, name: 'Weekend' })).json;
    return { env, token, profile, list };
  }

  it('stores the title once and returns it with the year in lists and statuses', async () => {
    const { env, token, profile, list } = await setup();
    const meta = { title: 'Heat', poster: 'https://img.example/heat.jpg', year: 1995, media_type: 'movie' };
    await call(env, token, 'POST', '/v1/profiles/lists/items', { list_id: list.id, catalog_id: 'tt1', ...meta });
    await call(env, token, 'POST', '/v1/profiles/status', { profile_id: profile.id, catalog_id: 'tt1', status: 'watched' });

    const lists = (await call(env, token, 'GET', `/v1/profiles/lists?profile_id=${profile.id}`)).json;
    expect(lists.lists[0].items[0]).toMatchObject({ title: 'Heat', year: 1995, poster: meta.poster });
    const status = (await call(env, token, 'GET', `/v1/profiles/status?profile_id=${profile.id}`)).json;
    expect(status.watch_status[0]).toMatchObject({ title: 'Heat', year: 1995, media_type: 'movie' });
  });

  it('drops a poster that is not a web URL and an out-of-range year', async () => {
    const { env, token, profile, list } = await setup();
    await call(env, token, 'POST', '/v1/profiles/lists/items', {
      list_id: list.id,
      catalog_id: 'tt1',
      media_type: 'movie',
      title: 'Heat',
      poster: 'javascript:alert(1)',
      year: 99999,
    });
    const lists = (await call(env, token, 'GET', `/v1/profiles/lists?profile_id=${profile.id}`)).json;
    expect(lists.lists[0].items[0]).toMatchObject({ title: 'Heat', poster: null, year: null });
  });

  it('keeps title data private to one invite', async () => {
    const a = await setup('invite-a');
    const b = await registerDevice(a.env, 'invite-b');
    const profileB = (await call(a.env, b.token, 'POST', '/v1/profiles', { name: 'Bo', avatar_key: 'rose' })).json;
    const listB = (await call(a.env, b.token, 'POST', '/v1/profiles/lists', { profile_id: profileB.id, name: 'Mine' })).json;
    await call(a.env, a.token, 'POST', '/v1/profiles/lists/items', {
      list_id: a.list.id,
      catalog_id: 'tt1',
      media_type: 'movie',
      title: 'Heat',
    });
    await call(a.env, b.token, 'POST', '/v1/profiles/lists/items', {
      list_id: listB.id,
      catalog_id: 'tt1',
      media_type: 'movie',
    });
    const lists = (await call(a.env, b.token, 'GET', `/v1/profiles/lists?profile_id=${profileB.id}`)).json;
    expect(lists.lists[0].items[0].title).toBeNull();
  });

  it('refuses a second list with the same name in any letter case', async () => {
    const { env, token, profile } = await setup();
    const dup = await call(env, token, 'POST', '/v1/profiles/lists', { profile_id: profile.id, name: 'WEEKEND' });
    expect(dup.status).toBe(409);
    const other = (await call(env, token, 'POST', '/v1/profiles', { name: 'Bo', avatar_key: 'rose' })).json;
    const ok = await call(env, token, 'POST', '/v1/profiles/lists', { profile_id: other.id, name: 'Weekend' });
    expect(ok.status).toBe(200);
  });

  it('removes a watch status through the explicit route and records the deletion', async () => {
    const { env, token, profile } = await setup();
    await call(env, token, 'POST', '/v1/profiles/status', { profile_id: profile.id, catalog_id: 'tt1', status: 'watched' });
    const removed = await call(env, token, 'POST', '/v1/profiles/status/delete', {
      profile_id: profile.id,
      catalog_id: 'tt1',
    });
    expect(removed.status).toBe(200);
    const status = (await call(env, token, 'GET', `/v1/profiles/status?profile_id=${profile.id}`)).json;
    expect(status.watch_status).toHaveLength(0);
    const deletions = (await call(env, token, 'GET', '/v1/profiles/deletions')).json.deletions;
    expect(deletions).toContainEqual(
      expect.objectContaining({ kind: 'watch_status', key: `${profile.id}:tt1` })
    );
  });

  it('records deletions for items, lists, and profiles, and clears them on re-add', async () => {
    const { env, token, profile, list } = await setup();
    await call(env, token, 'POST', '/v1/profiles/lists/items', { list_id: list.id, catalog_id: 'tt1', media_type: 'movie' });
    await call(env, token, 'POST', '/v1/profiles/lists/items/delete', { list_id: list.id, catalog_id: 'tt1' });
    let deletions = (await call(env, token, 'GET', '/v1/profiles/deletions')).json.deletions;
    expect(deletions.map((d: any) => d.kind)).toContain('list_item');

    await call(env, token, 'POST', '/v1/profiles/lists/items', { list_id: list.id, catalog_id: 'tt1', media_type: 'movie' });
    deletions = (await call(env, token, 'GET', '/v1/profiles/deletions')).json.deletions;
    expect(deletions.map((d: any) => d.kind)).not.toContain('list_item');

    await call(env, token, 'POST', '/v1/profiles/lists/delete', { list_id: list.id });
    await call(env, token, 'POST', '/v1/profiles/delete', { profile_id: profile.id });
    deletions = (await call(env, token, 'GET', '/v1/profiles/deletions')).json.deletions;
    expect(deletions.map((d: any) => d.kind).sort()).toEqual(['list', 'profile']);
  });

  it('lists deletions for the caller invite only', async () => {
    const a = await setup('invite-a');
    const b = await registerDevice(a.env, 'invite-b');
    await call(a.env, a.token, 'POST', '/v1/profiles/lists/delete', { list_id: a.list.id });
    const seen = (await call(a.env, b.token, 'GET', '/v1/profiles/deletions')).json.deletions;
    expect(seen).toHaveLength(0);
  });

  it('keeps a client-supplied added_at and updated_at so a synced item keeps its age', async () => {
    const { env, token, profile, list } = await setup();
    const past = Date.parse('2024-01-01T00:00:00Z');
    await call(env, token, 'POST', '/v1/profiles/lists/items', {
      list_id: list.id,
      catalog_id: 'tt1',
      media_type: 'movie',
      added_at: past,
    });
    await call(env, token, 'POST', '/v1/profiles/status', {
      profile_id: profile.id,
      catalog_id: 'tt1',
      status: 'watched',
      updated_at: past,
    });
    const lists = (await call(env, token, 'GET', `/v1/profiles/lists?profile_id=${profile.id}`)).json;
    expect(lists.lists[0].items[0].added_at).toBe(past);
    const status = (await call(env, token, 'GET', `/v1/profiles/status?profile_id=${profile.id}`)).json;
    expect(status.watch_status[0].updated_at).toBe(past);
  });

  it('does not add a second list item for an alias id of the same title', async () => {
    const { env, token, profile, list } = await setup();
    await call(env, token, 'POST', '/v1/profiles/lists/items', {
      list_id: list.id,
      catalog_id: 'tt99',
      media_type: 'series',
      title: 'Show',
    });
    const alias = await call(env, token, 'POST', '/v1/profiles/lists/items', {
      list_id: list.id,
      catalog_id: 'kitsu:7',
      media_type: 'anime',
      canonical_id: 'tt99',
    });
    expect(alias.json.added).toBe(false);
    const lists = (await call(env, token, 'GET', `/v1/profiles/lists?profile_id=${profile.id}`)).json;
    expect(lists.lists[0].items).toHaveLength(1);
  });

  it('replaces a status stored under an alias id, and keeps a newer one', async () => {
    const { env, token, profile } = await setup();
    await call(env, token, 'POST', '/v1/profiles/status', {
      profile_id: profile.id,
      catalog_id: 'kitsu:7',
      status: 'to_watch',
      canonical_id: 'tt99',
    });
    await call(env, token, 'POST', '/v1/profiles/status', {
      profile_id: profile.id,
      catalog_id: 'tt99',
      status: 'watched',
    });
    let rows = (await call(env, token, 'GET', `/v1/profiles/status?profile_id=${profile.id}`)).json.watch_status;
    expect(rows.map((r: any) => r.catalog_id)).toEqual(['tt99']);

    const stale = await call(env, token, 'POST', '/v1/profiles/status', {
      profile_id: profile.id,
      catalog_id: 'kitsu:7',
      status: 'to_watch',
      canonical_id: 'tt99',
      updated_at: Date.parse('2024-01-01T00:00:00Z'),
    });
    expect(stale.json.skipped).toBe(true);
    rows = (await call(env, token, 'GET', `/v1/profiles/status?profile_id=${profile.id}`)).json.watch_status;
    expect(rows.map((r: any) => r.catalog_id)).toEqual(['tt99']);
  });
});

describe('Gateway migrations', () => {
  function applyMigrations(db: any, from: string, to: string) {
    const dir = fileURLToPath(new URL('../migrations/', import.meta.url).href);
    const files = readdirSync(dir)
      .filter((name) => name.endsWith('.sql'))
      .sort()
      .filter((name) => name >= from && name <= to);
    for (const file of files) db.exec(readFileSync(dir + file, 'utf8'));
  }

  it('moves copied title columns into the titles table and renames duplicate list names', () => {
    const db = new DatabaseSync(':memory:');
    applyMigrations(db, '0001', '0005~');
    db.exec(`
      INSERT INTO invites (code_hash, created_at) VALUES ('inv', 1);
      INSERT INTO profiles VALUES ('p1', 'inv', 'Ada', 'amber', 1, 1);
      INSERT INTO lists VALUES ('aaaa1111', 'p1', 'Weekend', 1);
      INSERT INTO lists VALUES ('bbbb2222', 'p1', 'weekend', 2);
      INSERT INTO list_items VALUES ('aaaa1111', 'tt1', 'movie', 5, 'Heat', 'https://img/h.jpg');
      INSERT INTO watch_status VALUES ('p1', 'tt1', 'watched', NULL, NULL, 9, NULL, NULL, 'movie');
      INSERT INTO watch_status VALUES ('p1', 'tt2', 'watched', NULL, NULL, 9, 'Alien', NULL, 'movie');
    `);
    applyMigrations(db, '0006', '0008~');

    const titles = db.prepare('SELECT catalog_id, title, poster, media_type FROM titles ORDER BY catalog_id').all();
    expect(titles).toEqual([
      { catalog_id: 'tt1', title: 'Heat', poster: 'https://img/h.jpg', media_type: 'movie' },
      { catalog_id: 'tt2', title: 'Alien', poster: null, media_type: 'movie' },
    ]);
    const names = db.prepare('SELECT name FROM lists ORDER BY created_at').all();
    expect(names).toEqual([{ name: 'Weekend' }, { name: 'weekend (bbbb)' }]);
    expect(() => db.exec("INSERT INTO lists VALUES ('c', 'p1', 'WEEKEND', 3)")).toThrow(/UNIQUE/);
    const columns = db.prepare('PRAGMA table_info(list_items)').all().map((c: any) => c.name);
    expect(columns).not.toContain('title');
  });
});
