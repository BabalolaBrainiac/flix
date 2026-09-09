import { describe, it, expect, vi } from 'vitest';
import { createRequire } from 'node:module';

// node:sqlite is newer than the bundler's built-in module list, so it is loaded
// at run time instead of through a static import.
const { DatabaseSync } = createRequire(import.meta.url)('node:sqlite');
import { generateSecureToken, sha256Hex } from '../src/crypto';
import type { Env } from '../src/types';
import worker from '../src/index';

// A SQLite-backed env with the three real tables, matching the pattern used in
// auth_and_invites.test.ts. Supports both first() and run().
function fullSqliteEnv(): Env {
  const db = new DatabaseSync(':memory:');
  db.exec(
    'CREATE TABLE rate_limits (key TEXT PRIMARY KEY, window_start INTEGER NOT NULL, count INTEGER NOT NULL);'
  );
  db.exec(
    'CREATE TABLE devices (token_hash TEXT PRIMARY KEY, invite_code_hash TEXT, created_at INTEGER, last_used_at INTEGER, is_revoked INTEGER DEFAULT 0, request_count INTEGER DEFAULT 0);'
  );
  db.exec(
    'CREATE TABLE invites (code_hash TEXT PRIMARY KEY, created_at INTEGER, max_devices INTEGER, redeemed_count INTEGER DEFAULT 0, is_revoked INTEGER DEFAULT 0);'
  );

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
          };
        },
      };
    },
  };

  return { DB, OPENSUBTITLES_COORDINATOR: {} as any };
}

const ctx = {} as ExecutionContext;

describe('Catalog and Reader route auth', () => {
  it('/v1/catalog/search returns 401 without auth', async () => {
    const env = fullSqliteEnv();
    const res = await worker.fetch(
      new Request('https://gateway/v1/catalog/search?q=test'),
      env,
      ctx
    );
    expect(res.status).toBe(401);
  });

  it('/v1/catalog/discover returns 401 without auth', async () => {
    const env = fullSqliteEnv();
    const res = await worker.fetch(
      new Request('https://gateway/v1/catalog/discover'),
      env,
      ctx
    );
    expect(res.status).toBe(401);
  });

  it('/v1/reader/search returns 401 without auth', async () => {
    const env = fullSqliteEnv();
    const res = await worker.fetch(
      new Request('https://gateway/v1/reader/search?q=test'),
      env,
      ctx
    );
    expect(res.status).toBe(401);
  });

  it('/v1/reader/chapters returns 401 without auth', async () => {
    const env = fullSqliteEnv();
    const res = await worker.fetch(
      new Request('https://gateway/v1/reader/chapters?manga_id=some-id'),
      env,
      ctx
    );
    expect(res.status).toBe(401);
  });

  it.each([
    '/v1/reader/chapters?manga_id=not-a-valid-uuid',
    '/v1/reader/chapters?manga_id=' + '-'.repeat(36),
    '/v1/reader/search?q=test&limit=0',
    '/v1/reader/search?q=test&limit=bad',
    '/v1/catalog/search?q=' + 'x'.repeat(201),
    '/v1/catalog/discover?skip=-1',
    '/v1/catalog/discover?skip=bad',
  ])('rejects invalid input at %s', async (path) => {
    const env = fullSqliteEnv();
    const token = generateSecureToken(32);
    const tokenHash = await sha256Hex(token);

    // Insert a valid device so auth passes
    env.DB.prepare(
      'INSERT INTO devices (token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count) VALUES (?, ?, ?, ?, 0, 0)'
    )
      .bind(tokenHash, 'invite-hash', Date.now(), Date.now())
      .run();

    const res = await worker.fetch(
      new Request(`https://gateway${path}`, {
        headers: { Authorization: `Bearer ${token}` },
      }),
      env,
      ctx
    );
    expect(res.status).toBe(400);
  });
});
