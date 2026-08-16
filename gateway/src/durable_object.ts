import type { Env } from './types';

interface CachedSession {
  token: string;
  expiresAt: number;
}

export class OpenSubtitlesCoordinator {
  private state: DurableObjectState;
  private env: Env;
  private lastLoginAttempt = 0;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.env = env;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/token' && request.method === 'POST') {
      return this.handleGetToken(request);
    }

    return new Response('Not Found', { status: 404 });
  }

  private async handleGetToken(request: Request): Promise<Response> {
    const now = Date.now();
    const cached = await this.state.storage.get<CachedSession>('session');

    // Return cached token if valid for at least another 10 minutes
    if (cached && cached.expiresAt > now + 10 * 60 * 1000) {
      return Response.json({ token: cached.token, cached: true });
    }

    // Rate-limit login attempts to once every 3 seconds
    if (now - this.lastLoginAttempt < 3000) {
      return Response.json(
        { error: 'Login attempt throttled. Try again shortly.' },
        { status: 429 }
      );
    }
    this.lastLoginAttempt = now;

    const apiKey = this.env.OPENSUBTITLES_API_KEY;
    const userAgent = this.env.OPENSUBTITLES_USER_AGENT || 'Flix Gateway v0.1.0';
    const username = this.env.OPENSUBTITLES_USERNAME;
    const password = this.env.OPENSUBTITLES_PASSWORD;

    if (!apiKey || !username || !password) {
      return Response.json(
        { error: 'OpenSubtitles credentials are not configured on gateway' },
        { status: 503 }
      );
    }

    try {
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
        return Response.json(
          { error: `OpenSubtitles login failed with status ${response.status}` },
          { status: 502 }
        );
      }

      const data: any = await response.json();
      const token = data.token;
      if (!token) {
        return Response.json(
          { error: 'OpenSubtitles response missing token' },
          { status: 502 }
        );
      }

      // Default OpenSubtitles tokens last 24h, cache for 20h
      const session: CachedSession = {
        token,
        expiresAt: now + 20 * 60 * 60 * 1000,
      };
      await this.state.storage.put('session', session);

      return Response.json({ token, cached: false });
    } catch (e: any) {
      return Response.json(
        { error: `OpenSubtitles login exception: ${e.message}` },
        { status: 502 }
      );
    }
  }
}
