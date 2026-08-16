import { describe, it, expect, vi } from 'vitest';
import { generateSecureToken, sha256Hex } from '../src/crypto';
import { authenticateAdmin, authenticateDevice } from '../src/auth';
import { handleRedeemInvite } from '../src/routes/invites';
import type { Env, InviteRecord } from '../src/types';

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

describe('Gateway Admin Auth', () => {
  const env: Env = {
    DB: {} as any,
    OPENSUBTITLES_COORDINATOR: {} as any,
    ADMIN_SECRET: 'super-secret-admin-key',
  };

  it('authorizes admin with correct header', () => {
    const req = new Request('http://gateway/v1/admin/invites', {
      headers: { 'X-Admin-Secret': 'super-secret-admin-key' },
    });
    expect(authenticateAdmin(req, env)).toBe(true);
  });

  it('rejects admin with missing or invalid header', () => {
    const req = new Request('http://gateway/v1/admin/invites', {
      headers: { 'X-Admin-Secret': 'wrong-key' },
    });
    expect(authenticateAdmin(req, env)).toBe(false);
  });
});

describe('Gateway Device Auth & Invite Redemption', () => {
  it('rejects device authentication if token is missing', async () => {
    const env: Env = {
      DB: {} as any,
      OPENSUBTITLES_COORDINATOR: {} as any,
    };
    const req = new Request('http://gateway/v1/catalog/search');
    const res = await authenticateDevice(req, env);
    expect(res).toBeInstanceOf(Response);
    if (res instanceof Response) {
      expect(res.status).toBe(401);
    }
  });

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
          first: vi.fn().mockResolvedValue(mockInvite),
        }),
      })),
      batch: vi.fn().mockResolvedValue([
        { success: true, meta: { changes: 1 } },
        { success: true, meta: { changes: 1 } },
      ]),
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

  it('rejects invite redemption if max_devices is reached', async () => {
    const mockInvite: InviteRecord = {
      code_hash: await sha256Hex('used_invite'),
      created_at: Date.now(),
      max_devices: 1,
      redeemed_count: 1,
      is_revoked: 0,
    };

    const mockDB: any = {
      prepare: vi.fn().mockImplementation(() => ({
        bind: vi.fn().mockReturnValue({
          first: vi.fn().mockResolvedValue(mockInvite),
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
});
