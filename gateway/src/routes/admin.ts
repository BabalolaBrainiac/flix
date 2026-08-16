import type { Env } from '../types';
import { authenticateAdmin } from '../auth';
import { generateSecureToken, sha256Hex } from '../crypto';

export async function handleCreateInvite(request: Request, env: Env): Promise<Response> {
  if (!authenticateAdmin(request, env)) {
    return Response.json({ error: 'Unauthorized admin access' }, { status: 401 });
  }

  let body: any = {};
  try {
    body = await request.json();
  } catch {
    // Body is optional
  }

  const rawCode = typeof body.invite_code === 'string' && body.invite_code.trim()
    ? body.invite_code.trim()
    : `flix_${generateSecureToken(8)}`;
  const maxDevices = typeof body.max_devices === 'number' && body.max_devices > 0 ? body.max_devices : 1;

  const codeHash = await sha256Hex(rawCode);
  const now = Date.now();

  try {
    await env.DB.prepare(
      'INSERT INTO invites (code_hash, created_at, max_devices, redeemed_count, is_revoked) VALUES (?, ?, ?, 0, 0)'
    )
      .bind(codeHash, now, maxDevices)
      .run();

    return Response.json({
      invite_code: rawCode,
      max_devices: maxDevices,
      created_at: now,
    });
  } catch (e: any) {
    return Response.json({ error: `Failed to create invite: ${e.message}` }, { status: 400 });
  }
}

export async function handleRevokeDevice(request: Request, env: Env): Promise<Response> {
  if (!authenticateAdmin(request, env)) {
    return Response.json({ error: 'Unauthorized admin access' }, { status: 401 });
  }

  let body: any;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: 'Invalid JSON payload' }, { status: 400 });
  }

  const tokenHash = typeof body.token_hash === 'string' ? body.token_hash.trim() : '';
  const rawInviteCode = typeof body.invite_code === 'string' ? body.invite_code.trim() : '';

  if (tokenHash) {
    await env.DB.prepare('UPDATE devices SET is_revoked = 1 WHERE token_hash = ?')
      .bind(tokenHash)
      .run();
    return Response.json({ success: true, revoked_token_hash: tokenHash });
  }

  if (rawInviteCode) {
    const codeHash = await sha256Hex(rawInviteCode);
    await env.DB.batch([
      env.DB.prepare('UPDATE invites SET is_revoked = 1 WHERE code_hash = ?').bind(codeHash),
      env.DB.prepare('UPDATE devices SET is_revoked = 1 WHERE invite_code_hash = ?').bind(codeHash),
    ]);
    return Response.json({ success: true, revoked_invite_code_hash: codeHash });
  }

  return Response.json({ error: 'Provide either token_hash or invite_code' }, { status: 400 });
}
