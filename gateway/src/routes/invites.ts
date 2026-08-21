import type { Env, InviteRecord } from '../types';
import { authenticateDevice, checkRateLimit, getClientIp } from '../auth';
import { generateSecureToken, sha256Hex } from '../crypto';

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000; // 7 days in milliseconds

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
  const now = Date.now();

  // Per-IP limit on redemption attempts. The per-code limit below keys on the
  // submitted code, so a guesser gets a fresh bucket for every guess. This
  // limit keys on the caller instead, so it caps guessing across many codes.
  const ip = getClientIp(request);
  const ipAllowed = await checkRateLimit(env, `invite-ip:${ip}`, 10, 60);
  if (!ipAllowed) {
    return Response.json({ error: 'Invite redemption rate limit exceeded' }, { status: 429 });
  }

  const rateLimited = await checkRateLimit(env, `invite:${codeHash}`, 5, 60);
  if (!rateLimited) {
    return Response.json({ error: 'Invite redemption rate limit exceeded' }, { status: 429 });
  }

  const invite = await env.DB.prepare(
    'SELECT code_hash, created_at, max_devices, redeemed_count, is_revoked FROM invites WHERE code_hash = ?'
  )
    .bind(codeHash)
    .first<InviteRecord>();

  if (!invite || invite.is_revoked === 1) {
    return Response.json({ error: 'Invite code is invalid or revoked' }, { status: 403 });
  }

  // Check 7-day expiry
  if (now - invite.created_at > SEVEN_DAYS_MS) {
    return Response.json({ error: 'Invite code has expired' }, { status: 403 });
  }

  if (invite.redeemed_count >= invite.max_devices) {
    return Response.json(
      { error: 'Invite code has already been redeemed on the maximum allowed devices' },
      { status: 403 }
    );
  }

  // Generate 256-bit secure raw device token to return to the client
  const rawDeviceToken = generateSecureToken(32);
  const tokenHash = await sha256Hex(rawDeviceToken);

  const updateResult = await env.DB.prepare(
    `
      UPDATE invites
      SET redeemed_count = redeemed_count + 1
      WHERE code_hash = ?
        AND is_revoked = 0
        AND created_at >= ?
        AND redeemed_count < max_devices
    `
  )
    .bind(codeHash, now - SEVEN_DAYS_MS)
    .run();

  if (!updateResult.success || updateResult.meta.changes === 0) {
    return Response.json(
      { error: 'Failed to redeem invite. Maximum device limit reached.' },
      { status: 403 }
    );
  }

  const insertResult = await env.DB.prepare(
    `
      INSERT INTO devices
        (token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count)
      VALUES (?, ?, ?, ?, 0, 0)
    `
  )
    .bind(tokenHash, codeHash, now, now)
    .run();

  if (!insertResult.success || insertResult.meta.changes === 0) {
    await env.DB.prepare(
      `
        UPDATE invites
        SET redeemed_count = CASE
          WHEN redeemed_count > 0 THEN redeemed_count - 1
          ELSE 0
        END
        WHERE code_hash = ?
      `
    )
      .bind(codeHash)
      .run()
      .catch(() => undefined);
    return Response.json({ error: 'Failed to register the activated device' }, { status: 500 });
  }

  return Response.json({
    device_token: rawDeviceToken,
    created_at: now,
  });
}

export async function handleDeviceStatus(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  return Response.json({
    status: 'active',
    created_at: authRes.device.created_at,
    last_used_at: authRes.device.last_used_at,
  });
}
