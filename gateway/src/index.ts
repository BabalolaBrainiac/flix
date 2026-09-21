import type { Env } from './types';
import { handleRedeemInvite, handleDeviceStatus } from './routes/invites';
import { handleSubtitlesResolve } from './routes/subtitles';
import { handleCatalogSearch, handleCatalogTitle, handleCatalogDiscover } from './routes/catalog';
import { handleReaderSearch, handleReaderChapters } from './routes/reader';
import {
  handleListProfiles,
  handleCreateProfile,
  handleUpdateProfile,
  handleDeleteProfile,
  handleListLists,
  handleCreateList,
  handleDeleteList,
  handleAddListItem,
  handleRemoveListItem,
  handleListWatchStatus,
  handleSetWatchStatus,
  handleRemoveWatchStatus,
  handleListDeletions,
} from './routes/profiles';
import {
  requireAdmin,
  adminPageHtml,
  handleAdminCreateInvite,
  handleAdminListInvites,
  handleAdminRevokeInvite,
  handleAdminRegenerateInvite,
  handleAdminListDevices,
  handleAdminRevokeDevice,
} from './routes/admin';
import { checkRateLimit, getClientIp } from './auth';

export { OpenSubtitlesCoordinator } from './durable_object';

// Largest request body the gateway reads. Every endpoint takes a small JSON
// object (an invite code, an IMDb id, two integers). 64 KiB is far above the
// real need and stops a large-body flood before any parsing.
const MAX_REQUEST_BODY = 64 * 1024;

// Global per-IP request budget across all endpoints except health. This is the
// edge that the zone does not yet provide for this hostname: it caps a flood
// from one address and short-circuits before any device or invite lookup, so an
// unauthenticated caller cannot force an unbounded number of database reads.
const IP_LIMIT_PER_MINUTE = 60;

// Headers added to every response. The gateway serves only JSON to a native
// client, so these are defense in depth: force HTTPS, stop content-type
// sniffing, and deny framing.
const SECURITY_HEADERS: Record<string, string> = {
  'Strict-Transport-Security': 'max-age=63072000; includeSubDomains; preload',
  'X-Content-Type-Options': 'nosniff',
  'X-Frame-Options': 'DENY',
  'Referrer-Policy': 'no-referrer',
  'Cache-Control': 'no-store',
};

function withSecurityHeaders(response: Response): Response {
  for (const [key, value] of Object.entries(SECURITY_HEADERS)) {
    response.headers.set(key, value);
  }
  return response;
}

async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);

  if (url.pathname === '/v1/health' && request.method === 'GET') {
    return Response.json({ status: 'ok', service: 'flix-gateway' });
  }

  // Reject an oversized body before the route reads it. A declared length above
  // the cap is refused outright; the fetch handler already read the body for a
  // real request, so this guards the common, honest case cheaply.
  const declaredLength = Number(request.headers.get('content-length') || 0);
  if (declaredLength > MAX_REQUEST_BODY) {
    return Response.json({ error: 'Request body too large' }, { status: 413 });
  }

  // Per-IP budget for every non-health route. This runs before authentication,
  // so a token flood is throttled before it reaches the device lookup.
  const ip = getClientIp(request);
  const withinBudget = await checkRateLimit(env, `ip:${ip}`, IP_LIMIT_PER_MINUTE, 60);
  if (!withinBudget) {
    return Response.json({ error: 'Rate limit exceeded' }, { status: 429 });
  }

  if (url.pathname === '/v1/invites/redeem' && request.method === 'POST') {
    return await handleRedeemInvite(request, env);
  }

  if (url.pathname === '/v1/devices/status' && request.method === 'GET') {
    return await handleDeviceStatus(request, env);
  }

  if (url.pathname === '/v1/subtitles/resolve' && request.method === 'POST') {
    return await handleSubtitlesResolve(request, env);
  }

  if (url.pathname === '/v1/catalog/search' && request.method === 'GET') {
    return await handleCatalogSearch(request, env);
  }

  if (url.pathname === '/v1/catalog/title' && request.method === 'GET') {
    return await handleCatalogTitle(request, env);
  }

  if (url.pathname === '/v1/catalog/discover' && request.method === 'GET') {
    return await handleCatalogDiscover(request, env);
  }

  if (url.pathname === '/v1/reader/search' && request.method === 'GET') {
    return await handleReaderSearch(request, env);
  }

  if (url.pathname === '/v1/reader/chapters' && request.method === 'GET') {
    return await handleReaderChapters(request, env);
  }

  if (url.pathname === '/v1/profiles' && request.method === 'GET') {
    return await handleListProfiles(request, env);
  }

  if (url.pathname === '/v1/profiles' && request.method === 'POST') {
    return await handleCreateProfile(request, env);
  }

  if (url.pathname === '/v1/profiles/update' && request.method === 'POST') {
    return await handleUpdateProfile(request, env);
  }

  if (url.pathname === '/v1/profiles/delete' && request.method === 'POST') {
    return await handleDeleteProfile(request, env);
  }

  if (url.pathname === '/v1/profiles/lists' && request.method === 'GET') {
    return await handleListLists(request, env);
  }

  if (url.pathname === '/v1/profiles/lists' && request.method === 'POST') {
    return await handleCreateList(request, env);
  }

  if (url.pathname === '/v1/profiles/lists/delete' && request.method === 'POST') {
    return await handleDeleteList(request, env);
  }

  if (url.pathname === '/v1/profiles/lists/items' && request.method === 'POST') {
    return await handleAddListItem(request, env);
  }

  if (url.pathname === '/v1/profiles/lists/items/delete' && request.method === 'POST') {
    return await handleRemoveListItem(request, env);
  }

  if (url.pathname === '/v1/profiles/status' && request.method === 'GET') {
    return await handleListWatchStatus(request, env);
  }

  if (url.pathname === '/v1/profiles/status' && request.method === 'POST') {
    return await handleSetWatchStatus(request, env);
  }

  if (url.pathname === '/v1/profiles/status/delete' && request.method === 'POST') {
    return await handleRemoveWatchStatus(request, env);
  }

  if (url.pathname === '/v1/profiles/deletions' && request.method === 'GET') {
    return await handleListDeletions(request, env);
  }

  // Admin page. The HTML carries no secret; the page asks for it and sends it
  // as a bearer token on each API call below.
  if (url.pathname === '/v1/admin' && request.method === 'GET') {
    return new Response(adminPageHtml(), {
      headers: { 'Content-Type': 'text/html; charset=utf-8' },
    });
  }

  if (url.pathname.startsWith('/v1/admin/')) {
    const denied = await requireAdmin(request, env);
    if (denied) return denied;

    if (url.pathname === '/v1/admin/invites' && request.method === 'GET') {
      return await handleAdminListInvites(env);
    }
    if (url.pathname === '/v1/admin/invites' && request.method === 'POST') {
      return await handleAdminCreateInvite(request, env);
    }
    if (url.pathname === '/v1/admin/invites/revoke' && request.method === 'POST') {
      return await handleAdminRevokeInvite(request, env);
    }
    if (url.pathname === '/v1/admin/invites/regenerate' && request.method === 'POST') {
      return await handleAdminRegenerateInvite(request, env);
    }
    if (url.pathname === '/v1/admin/devices' && request.method === 'GET') {
      return await handleAdminListDevices(env);
    }
    if (url.pathname === '/v1/admin/devices/revoke' && request.method === 'POST') {
      return await handleAdminRevokeDevice(request, env);
    }
  }

  return Response.json({ error: 'Endpoint not found' }, { status: 404 });
}

export default {
  async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
    try {
      return withSecurityHeaders(await route(request, env));
    } catch {
      return withSecurityHeaders(
        Response.json({ error: 'Internal gateway error' }, { status: 500 })
      );
    }
  },
};
