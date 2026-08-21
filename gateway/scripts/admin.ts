import { generateSecureToken, sha256Hex } from '../src/crypto';

// Owner-only helper to generate an invite code and its SQL insert command
export async function createInviteSql(maxDevices = 1): Promise<{ code: string; hash: string; sql: string }> {
  const code = generateSecureToken(32); // 256-bit token
  const hash = await sha256Hex(code);
  const now = Date.now();
  const sql = `INSERT INTO invites (code_hash, created_at, max_devices, redeemed_count, is_revoked) VALUES ('${hash}', ${now}, ${maxDevices}, 0, 0);`;
  return { code, hash, sql };
}

// Owner-only helper to revoke a device by token
export async function revokeDeviceSql(token: string): Promise<{ tokenHash: string; sql: string }> {
  const tokenHash = await sha256Hex(token);
  const sql = `UPDATE devices SET is_revoked = 1 WHERE token_hash = '${tokenHash}';`;
  return { tokenHash, sql };
}
