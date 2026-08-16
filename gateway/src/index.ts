import type { Env } from './types';
import { handleRedeemInvite } from './routes/invites';
import { handleCatalogSearch, handleCatalogTitle } from './routes/catalog';
import { handleSubtitlesSearch, handleSubtitlesDownload } from './routes/subtitles';
import { handleCreateInvite, handleRevokeDevice } from './routes/admin';

export { OpenSubtitlesCoordinator } from './durable_object';

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    // CORS preflight handling
    if (request.method === 'OPTIONS') {
      return new Response(null, {
        headers: {
          'Access-Control-Allow-Origin': '*',
          'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
          'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-Device-Token, X-Admin-Secret',
          'Access-Control-Max-Age': '86400',
        },
      });
    }

    try {
      if (url.pathname === '/v1/health' && request.method === 'GET') {
        return Response.json({ status: 'ok', service: 'flix-gateway' });
      }

      if (url.pathname === '/v1/invites/redeem' && request.method === 'POST') {
        return await handleRedeemInvite(request, env);
      }

      if (url.pathname === '/v1/catalog/search' && request.method === 'GET') {
        return await handleCatalogSearch(request, env);
      }

      if (url.pathname === '/v1/catalog/title' && request.method === 'GET') {
        return await handleCatalogTitle(request, env);
      }

      if (url.pathname === '/v1/subtitles/search' && request.method === 'GET') {
        return await handleSubtitlesSearch(request, env);
      }

      if (url.pathname === '/v1/subtitles/download' && request.method === 'POST') {
        return await handleSubtitlesDownload(request, env);
      }

      if (url.pathname === '/v1/admin/invites' && request.method === 'POST') {
        return await handleCreateInvite(request, env);
      }

      if (url.pathname === '/v1/admin/devices/revoke' && request.method === 'POST') {
        return await handleRevokeDevice(request, env);
      }

      return Response.json({ error: 'Endpoint not found' }, { status: 404 });
    } catch (e: any) {
      return Response.json(
        { error: 'Internal gateway error' },
        { status: 500 }
      );
    }
  },
};
