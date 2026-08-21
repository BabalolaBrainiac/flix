import { DurableObject } from 'cloudflare:workers';
import type { Env } from './types';

interface CachedSession {
  token: string;
  baseUrl: string;
  expiresAt: number;
}

// The class must extend DurableObject, or the Worker cannot call its methods by
// RPC. Without it, `coordinator.getSession()` throws at call time. The base
// class provides `this.ctx` (the DurableObjectState) and `this.env`.
export class OpenSubtitlesCoordinator extends DurableObject<Env> {
  private lastLoginAttempt = 0;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
  }

  async getSession(): Promise<{ token: string; baseUrl: string; expiresAt: number }> {
    const now = Date.now();
    const cached = await this.ctx.storage.get<CachedSession>('session');

    // Return cached token if valid for at least another 10 minutes
    if (cached && cached.expiresAt > now + 10 * 60 * 1000) {
      return cached;
    }

    // Rate-limit login attempts to once every 3 seconds
    if (now - this.lastLoginAttempt < 3000) {
      throw new Error('Login attempt throttled. Try again shortly.');
    }
    this.lastLoginAttempt = now;

    const apiKey = this.env.OPENSUBTITLES_API_KEY;
    const userAgent = this.env.OPENSUBTITLES_USER_AGENT || 'Flix Gateway v0.1.0';
    const username = this.env.OPENSUBTITLES_USERNAME;
    const password = this.env.OPENSUBTITLES_PASSWORD;

    if (!apiKey || !username || !password) {
      throw new Error('OpenSubtitles credentials are not configured on gateway');
    }

    const response = await fetch('https://api.opensubtitles.com/api/v1/login', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Api-Key': apiKey,
        'User-Agent': userAgent,
      },
      body: JSON.stringify({ username, password }),
    });

    if (!response.ok) {
      throw new Error(`OpenSubtitles login failed with status ${response.status}`);
    }

    const data: any = await response.json();
    const token = data.token;
    const baseUrl = data.base_url || 'https://api.opensubtitles.com';

    if (!token) {
      throw new Error('OpenSubtitles response missing token');
    }

    // Default OpenSubtitles tokens last 24h, cache for 20h
    const session: CachedSession = {
      token,
      baseUrl,
      expiresAt: now + 20 * 60 * 60 * 1000,
    };
    await this.ctx.storage.put('session', session);

    return session;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/token' && request.method === 'POST') {
      try {
        const session = await this.getSession();
        return Response.json(session);
      } catch (err: any) {
        return Response.json({ error: err.message }, { status: 502 });
      }
    }

    return new Response('Not Found', { status: 404 });
  }
}
