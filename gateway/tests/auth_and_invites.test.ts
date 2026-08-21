import { describe, it, expect, vi } from 'vitest';
import { createRequire } from 'node:module';

// node:sqlite is newer than the bundler's built-in module list, so it is loaded
// at run time instead of through a static import.
const { DatabaseSync } = createRequire(import.meta.url)('node:sqlite');
import { generateSecureToken, sha256Hex } from '../src/crypto';
import { authenticateDevice, checkRateLimit } from '../src/auth';
import { handleRedeemInvite, handleDeviceStatus } from '../src/routes/invites';
import { handleSubtitlesResolve } from '../src/routes/subtitles';
import type { Env, InviteRecord } from '../src/types';
import worker from '../src/index';

describe('Gateway Crypto', () => {
  it('computes sha256 hex string deterministically', async () => {
    const hash1 = await sha256Hex('test_invite_code');
    const hash2 = await sha256Hex('test_invite_code');
    const hash3 = await sha256Hex('different_code');

    expect(hash1).toHaveLength(64);
    expect(hash1).toBe(hash2);
    expect(hash1).not.toBe(hash3);
  });

  it('generates secure random token with expected length', () => {
    const token = generateSecureToken(32);
    expect(token).toHaveLength(64);
    expect(/^[0-9a-f]{64}$/.test(token)).toBe(true);
  });
});

describe('Gateway Device Auth & Rate Limits', () => {
  it('rejects device authentication if token is missing', async () => {
    const env: Env = {
      DB: {} as any,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };
    const req = new Request('http://gateway/v1/devices/status');
    const res = await authenticateDevice(req, env);
    expect(res).toBeInstanceOf(Response);
    if (res instanceof Response) {
      expect(res.status).toBe(401);
    }
  });

  it('fails closed when rate limit storage fails', async () => {
    const mockDB: any = {
      prepare: vi.fn().mockImplementation(() => {
        throw new Error('D1 error');
      }),
    };
    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };
    const allowed = await checkRateLimit(env, 'test-key', 10);
    expect(allowed).toBe(false);
  });

  it('uses one atomic statement for rate limits', async () => {
    const mockFirst = vi.fn().mockResolvedValue({ count: 2 });
    const mockBind = vi.fn().mockReturnValue({ first: mockFirst });
    const mockPrepare = vi.fn().mockReturnValue({ bind: mockBind });
    const env: Env = {
      DB: { prepare: mockPrepare } as any,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const allowed = await checkRateLimit(env, 'test-key', 3);

    expect(allowed).toBe(true);
    expect(mockPrepare).toHaveBeenCalledTimes(1);
  });
});

describe('Gateway Rate Limit Enforcement', () => {
  // The fake D1 runs the real SQL against SQLite, so this test fails if the
  // statement ever stops incrementing the counter above the limit again.
  function sqliteEnv(): Env {
    const db = new DatabaseSync(':memory:');
    db.exec(
      'CREATE TABLE rate_limits (key TEXT PRIMARY KEY, window_start INTEGER NOT NULL, count INTEGER NOT NULL);'
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
            };
          },
        };
      },
    };

    return { DB, OPENSUBTITLES_COORDINATOR: {} as any };
  }

  it('allows requests up to the limit and rejects every request after it', async () => {
    const env = sqliteEnv();
    const results: boolean[] = [];

    for (let attempt = 0; attempt < 8; attempt += 1) {
      results.push(await checkRateLimit(env, 'device-abc', 5));
    }

    expect(results.slice(0, 5)).toEqual([true, true, true, true, true]);
    // Request 6 and every request after it must be rejected. A counter that
    // stops at the limit would leave all of these true.
    expect(results.slice(5)).toEqual([false, false, false]);
  });

  it('keeps separate counters for separate keys', async () => {
    const env = sqliteEnv();

    for (let attempt = 0; attempt < 5; attempt += 1) {
      expect(await checkRateLimit(env, 'device-a', 5)).toBe(true);
    }
    expect(await checkRateLimit(env, 'device-a', 5)).toBe(false);
    expect(await checkRateLimit(env, 'device-b', 5)).toBe(true);
  });

  it('starts a new counter in the next time window', async () => {
    const env = sqliteEnv();
    const realNow = Date.now;

    try {
      Date.now = () => 60_000;
      for (let attempt = 0; attempt < 5; attempt += 1) {
        expect(await checkRateLimit(env, 'device-c', 5, 60)).toBe(true);
      }
      expect(await checkRateLimit(env, 'device-c', 5, 60)).toBe(false);

      Date.now = () => 120_000;
      expect(await checkRateLimit(env, 'device-c', 5, 60)).toBe(true);
    } finally {
      Date.now = realNow;
    }
  });
});

describe('Gateway Invite Redemption', () => {
  it('redeems valid invite code and returns new device token', async () => {
    const mockInvite: InviteRecord = {
      code_hash: await sha256Hex('valid_invite'),
      created_at: Date.now(),
      max_devices: 1,
      redeemed_count: 0,
      is_revoked: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation((query: string) => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockResolvedValue(
            query.includes('SELECT code_hash') ? mockInvite : { count: 1 }
          ),
          run: vi.fn().mockResolvedValue({ success: true, meta: { changes: 1 } }),
        }),
      })),
    };

    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const req = new Request('http://gateway/v1/invites/redeem', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ invite_code: 'valid_invite' }),
    });

    const res = await handleRedeemInvite(req, env);
    expect(res.status).toBe(200);
    const body: any = await res.json();
    expect(body.device_token).toBeDefined();
    expect(body.device_token).toHaveLength(64);
  });

  it('rejects expired invite codes older than 7 days', async () => {
    const eightDaysAgo = Date.now() - 8 * 24 * 60 * 60 * 1000;
    const mockInvite: InviteRecord = {
      code_hash: await sha256Hex('expired_invite'),
      created_at: eightDaysAgo,
      max_devices: 1,
      redeemed_count: 0,
      is_revoked: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation((query: string) => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockResolvedValue(
            query.includes('FROM invites') ? mockInvite : { count: 1 }
          ),
        }),
      })),
    };

    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const req = new Request('http://gateway/v1/invites/redeem', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ invite_code: 'expired_invite' }),
    });

    const res = await handleRedeemInvite(req, env);
    expect(res.status).toBe(403);
    const body: any = await res.json();
    expect(body.error).toContain('expired');
  });

  it('rejects invite redemption if max_devices is reached', async () => {
    const mockInvite: InviteRecord = {
      code_hash: await sha256Hex('used_invite'),
      created_at: Date.now(),
      max_devices: 1,
      redeemed_count: 1,
      is_revoked: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation((query: string) => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockResolvedValue(
            query.includes('FROM invites') ? mockInvite : { count: 1 }
          ),
        }),
      })),
    };

    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const req = new Request('http://gateway/v1/invites/redeem', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ invite_code: 'used_invite' }),
    });

    const res = await handleRedeemInvite(req, env);
    expect(res.status).toBe(403);
  });

  it('rejects invite redemption when the atomic invite update changes zero rows', async () => {
    const mockInvite: InviteRecord = {
      code_hash: await sha256Hex('almost-used_invite'),
      created_at: Date.now(),
      max_devices: 1,
      redeemed_count: 0,
      is_revoked: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation((query: string) => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockResolvedValue(
            query.includes('SELECT code_hash') ? mockInvite : { count: 1 }
          ),
          run: vi.fn().mockResolvedValue(
            query.includes('SET redeemed_count = redeemed_count + 1')
              ? { success: true, meta: { changes: 0 } }
              : { success: true, meta: { changes: 1 } }
          ),
        }),
      })),
    };

    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const req = new Request('http://gateway/v1/invites/redeem', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ invite_code: 'almost-used_invite' }),
    });

    const res = await handleRedeemInvite(req, env);
    expect(res.status).toBe(403);
  });
});

describe('Gateway Subtitle Resolution API', () => {
  it('rejects resolve requests with forbidden parameters like magnet or hash', async () => {
    const token = generateSecureToken(32);
    const tokenHash = await sha256Hex(token);
    const mockDevice = {
      token_hash: tokenHash,
      invite_code_hash: 'invite-hash',
      created_at: Date.now(),
      last_used_at: Date.now(),
      is_revoked: 0,
      request_count: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation((query: string) => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockImplementation(async () => {
            if (query.includes('FROM devices')) return mockDevice;
            return { count: 1 };
          }),
          run: vi.fn().mockResolvedValue({ success: true }),
        }),
      })),
    };

    const env: Env = {
      DB: mockDB,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };

    const req = new Request('http://gateway/v1/subtitles/resolve', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({
        imdb_id: 'tt10161886',
        magnet: 'magnet:?xt=urn:btih:1234',
      }),
    });

    const res = await handleSubtitlesResolve(req, env);
    expect(res.status).toBe(400);
    const body: any = await res.json();
    expect(body.error).toContain('Forbidden parameter');
  });
});

describe('Gateway Fetch Handler Hardening', () => {
  // A SQLite-backed env with the three real tables. It supports both first()
  // and run(), so the whole fetch pipeline runs against a real database.
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

  it('adds security headers to every response', async () => {
    const env = fullSqliteEnv();
    const res = await worker.fetch(new Request('https://gateway/v1/health'), env, ctx);
    expect(res.status).toBe(200);
    expect(res.headers.get('Strict-Transport-Security')).toContain('max-age=');
    expect(res.headers.get('X-Content-Type-Options')).toBe('nosniff');
    expect(res.headers.get('X-Frame-Options')).toBe('DENY');
  });

  it('rejects a body larger than the cap with 413', async () => {
    const env = fullSqliteEnv();
    const req = new Request('https://gateway/v1/invites/redeem', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'Content-Length': String(128 * 1024) },
      body: JSON.stringify({ invite_code: 'x'.repeat(128 * 1024) }),
    });
    const res = await worker.fetch(req, env, ctx);
    expect(res.status).toBe(413);
  });

  it('applies a per-IP budget across requests and then returns 429', async () => {
    const env = fullSqliteEnv();
    const ip = '203.0.113.7';
    const codes: number[] = [];
    // devices/status with no token returns 401, but the per-IP limit runs
    // first. The 61st request in the window must flip to 429.
    for (let attempt = 0; attempt < 62; attempt += 1) {
      const req = new Request('https://gateway/v1/devices/status', {
        headers: { 'CF-Connecting-IP': ip },
      });
      const res = await worker.fetch(req, env, ctx);
      codes.push(res.status);
    }
    expect(codes.slice(0, 60).every((c) => c === 401)).toBe(true);
    expect(codes[60]).toBe(429);
    expect(codes[61]).toBe(429);
  });

  it('keeps a separate per-IP budget for a different IP', async () => {
    const env = fullSqliteEnv();
    // Exhaust one IP.
    for (let attempt = 0; attempt < 61; attempt += 1) {
      await worker.fetch(
        new Request('https://gateway/v1/devices/status', { headers: { 'CF-Connecting-IP': '198.51.100.1' } }),
        env,
        ctx
      );
    }
    // A different IP is unaffected.
    const res = await worker.fetch(
      new Request('https://gateway/v1/devices/status', { headers: { 'CF-Connecting-IP': '198.51.100.2' } }),
      env,
      ctx
    );
    expect(res.status).toBe(401);
  });
});
