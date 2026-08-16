import type { DeviceRecord, Env } from './types';
import { sha256Hex } from './crypto';

export async function authenticateDevice(
  request: Request,
  env: Env
): Promise<{ tokenHash: string; device: DeviceRecord } | Response> {
  const authHeader = request.headers.get('Authorization');
  const customHeader = request.headers.get('X-Device-Token');

  let rawToken: string | null = null;
  if (authHeader && authHeader.startsWith('Bearer ')) {
    rawToken = authHeader.substring(7).trim();
  } else if (customHeader) {
    rawToken = customHeader.trim();
  }

  if (!rawToken || rawToken.length < 16) {
    return Response.json({ error: 'Missing or invalid device token' }, { status: 401 });
  }

  const tokenHash = await sha256Hex(rawToken);

  const device = await env.DB.prepare(
    'SELECT token_hash, invite_code_hash, created_at, last_used_at, is_revoked, request_count FROM devices WHERE token_hash = ?'
  )
    .bind(tokenHash)
    .first<DeviceRecord>();

  if (!device || device.is_revoked === 1) {
    return Response.json({ error: 'Device token is invalid or revoked' }, { status: 401 });
  }

  // Update last_used_at and usage counter asynchronously
  const now = Date.now();
  env.DB.prepare(
    'UPDATE devices SET last_used_at = ?, request_count = request_count + 1 WHERE token_hash = ?'
  )
    .bind(now, tokenHash)
    .run()
    .catch(() => {
      // Ignore background counter update errors
    });

  return { tokenHash, device };
}

export function authenticateAdmin(request: Request, env: Env): boolean {
  const adminSecret = env.ADMIN_SECRET;
  if (!adminSecret) return false;

  const authHeader = request.headers.get('Authorization');
  const customHeader = request.headers.get('X-Admin-Secret');

  if (authHeader && authHeader === `Bearer ${adminSecret}`) {
    return true;
  }
  if (customHeader && customHeader === adminSecret) {
    return true;
  }

  return false;
}

export async function checkRateLimit(
  env: Env,
  key: string,
  limit: number,
  windowSeconds = 60
): Promise<boolean> {
  const now = Date.now();
  const windowStart = Math.floor(now / (windowSeconds * 1000)) * (windowSeconds * 1000);
  const rateKey = `${key}:${windowStart}`;

  try {
    const existing = await env.DB.prepare(
      'SELECT count FROM rate_limits WHERE key = ?'
    )
      .bind(rateKey)
      .first<{ count: number }>();

    if (existing) {
      if (existing.count >= limit) {
        return false;
      }
      await env.DB.prepare(
        'UPDATE rate_limits SET count = count + 1 WHERE key = ?'
      )
        .bind(rateKey)
        .run();
    } else {
      await env.DB.prepare(
        'INSERT INTO rate_limits (key, window_start, count) VALUES (?, ?, 1)'
      )
        .bind(rateKey, windowStart)
        .run();
    }
    return true;
  } catch {
    // Fail-open for transient rate-limit storage issues
    return true;
  }
}
