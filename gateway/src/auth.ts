import type { DeviceRecord, Env } from './types';
import { sha256Hex } from './crypto';

// Returns the caller IP that Cloudflare sets. The client cannot forge this
// header; Cloudflare overwrites it at the edge. It is the key for per-IP rate
// limits. A missing value falls back to a single shared bucket, which fails
// safe by throttling harder, not less.
export function getClientIp(request: Request): string {
  const ip = request.headers.get('CF-Connecting-IP');
  return ip && ip.trim() ? ip.trim() : 'unknown';
}

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
    // The counter must keep increasing above the limit. If it stops at the
    // limit, every later request in the window still reads count === limit and
    // stays allowed, so the limit never rejects anything.
    const result = await env.DB.prepare(
      `
        INSERT INTO rate_limits (key, window_start, count)
        VALUES (?, ?, 1)
        ON CONFLICT(key) DO UPDATE SET count = rate_limits.count + 1
        RETURNING count
      `
    )
      .bind(rateKey, windowStart)
      .first<{ count: number }>();

    return typeof result?.count === 'number' && result.count <= limit;
  } catch {
    // Fail closed on rate limit storage errors
    return false;
  }
}
