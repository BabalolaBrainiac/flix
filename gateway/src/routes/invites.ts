import type { Env, InviteRecord } from '../types';
import { generateSecureToken, sha256Hex } from '../crypto';

export async function handleRedeemInvite(request: Request, env: Env): Promise<Response> {
  let body: any;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: 'Invalid JSON payload' }, { status: 400 });
  }

  const rawInviteCode = typeof body.invite_code === 'string' ? body.invite_code.trim() : '';
  if (!rawInviteCode) {
    return Response.json({ error: 'Missing invite_code' }, { status: 400 });
  }

  const codeHash = await sha256Hex(rawInviteCode);

  const invite = await env.DB.prepare(
    'SELECT code_hash, created_at, max_devices, redeemed_count, is_revoked FROM invites WHERE code_hash = ?'
  )
    .bind(codeHash)
    .first<InviteRecord>();

  if (!invite || invite.is_revoked === 1) {
    return Response.json({ error: 'Invite code is invalid or revoked' }, { status: 403 });
  }

  if (invite.redeemed_count >= invite.max_devices) {
    return Response.json(
      { error: 'Invite code has already been redeemed on the maximum allowed devices' },
      { status: 403 }
    );
  }

  // Generate secure raw device token to return to the client
  const rawDeviceToken = generateSecureToken(32);
  const tokenHash = await sha256Hex(rawDeviceToken);
  const now = Date.now();

  // Atomically increment redeemed count and register device
  const batchRes = await env.DB.batch([
    env.DB.prepare(
      'UPDATE invites SET redeemed_count = redeemed_count + 1 WHERE code_hash = ? AND redeemed_count < max_devices'
    ).bind(codeHash),
    env.DB.prepare(
      'INSERT INTO devices (token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count) VALUES (?, ?, ?, ?, 0, 0)'
    ).bind(tokenHash, codeHash, now, now),
  ]);

  if (!batchRes[0].success || batchRes[0].meta.changes === 0) {
    return Response.json(
      { error: 'Failed to redeem invite. Maximum device limit reached.' },
      { status: 403 }
    );
  }

  return Response.json({
    device_token: rawDeviceToken,
    created_at: now,
  });
}
