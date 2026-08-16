import type { Env, SubtitleSearchResult } from '../types';
import { authenticateDevice, checkRateLimit } from '../auth';

export async function handleSubtitlesSearch(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `subsearch:${authRes.tokenHash}`, 40, 60);
  if (!rateOk) {
    return Response.json({ error: 'Subtitle search rate limit exceeded' }, { status: 429 });
  }

  const url = new URL(request.url);
  const query = (url.searchParams.get('query') || '').trim();
  const imdbId = (url.searchParams.get('imdb_id') || '').trim();
  const season = url.searchParams.get('season');
  const episode = url.searchParams.get('episode');

  const apiKey = env.OPENSUBTITLES_API_KEY;
  if (!apiKey) {
    return Response.json({ results: [], notes: ['OpenSubtitles is not configured on gateway'] });
  }

  const openSubtitlesUrl = new URL('https://api.opensubtitles.com/api/v1/subtitles');
  openSubtitlesUrl.searchParams.set('languages', 'en');
  openSubtitlesUrl.searchParams.set('order_by', 'download_count');
  openSubtitlesUrl.searchParams.set('order_direction', 'desc');

  if (imdbId) {
    const numericImdb = imdbId.replace(/^tt/, '');
    openSubtitlesUrl.searchParams.set('imdb_id', numericImdb);
  }
  if (query && !imdbId) {
    openSubtitlesUrl.searchParams.set('query', query);
  }
  if (season) {
    openSubtitlesUrl.searchParams.set('season_number', season);
  }
  if (episode) {
    openSubtitlesUrl.searchParams.set('episode_number', episode);
  }

  try {
    const userAgent = env.OPENSUBTITLES_USER_AGENT || 'Flix Gateway v0.1.0';
    const res = await fetch(openSubtitlesUrl.toString(), {
      headers: {
        'Api-Key': apiKey,
        'User-Agent': userAgent,
        Accept: 'application/json',
      },
    });

    if (!res.ok) {
      return Response.json(
        { error: `OpenSubtitles search failed (${res.status})` },
        { status: 502 }
      );
    }

    const data: any = await res.json();
    const results: SubtitleSearchResult[] = [];
    if (Array.isArray(data.data)) {
      for (const item of data.data) {
        const file = item.attributes?.files?.[0];
        if (file?.file_id) {
          results.push({
            id: item.id,
            file_id: file.file_id,
            file_name: item.attributes.release || item.attributes.feature_details?.movie_name,
            release: item.attributes.release,
            download_count: item.attributes.download_count || 0,
          });
        }
      }
    }

    return Response.json({ results });
  } catch (e: any) {
    return Response.json({ error: `Subtitle search error: ${e.message}` }, { status: 502 });
  }
}

export async function handleSubtitlesDownload(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `subdl:${authRes.tokenHash}`, 20, 60);
  if (!rateOk) {
    return Response.json({ error: 'Subtitle download rate limit exceeded' }, { status: 429 });
  }

  let body: any;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: 'Invalid JSON payload' }, { status: 400 });
  }

  const fileId = body.file_id;
  if (!fileId || typeof fileId !== 'number') {
    return Response.json({ error: 'Missing or invalid file_id' }, { status: 400 });
  }

  const apiKey = env.OPENSUBTITLES_API_KEY;
  const userAgent = env.OPENSUBTITLES_USER_AGENT || 'Flix Gateway v0.1.0';

  if (!apiKey) {
    return Response.json({ error: 'OpenSubtitles is not configured on gateway' }, { status: 503 });
  }

  // Request token from coordinator Durable Object
  const id = env.OPENSUBTITLES_COORDINATOR.idFromName('global_coordinator');
  const coordinator = env.OPENSUBTITLES_COORDINATOR.get(id);

  const tokenRes = await coordinator.fetch('https://coordinator/token', {
    method: 'POST',
  });

  if (!tokenRes.ok) {
    const errorData: any = await tokenRes.json().catch(() => ({}));
    return Response.json(
      { error: errorData.error || 'Failed to acquire OpenSubtitles download session' },
      { status: tokenRes.status }
    );
  }

  const tokenData: any = await tokenRes.json();
  const sessionToken = tokenData.token;

  try {
    const downloadRes = await fetch('https://api.opensubtitles.com/api/v1/download', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Api-Key': apiKey,
        Authorization: `Bearer ${sessionToken}`,
        'User-Agent': userAgent,
        Accept: 'application/json',
      },
      body: JSON.stringify({ file_id: fileId }),
    });

    if (!downloadRes.ok) {
      return Response.json(
        { error: `OpenSubtitles download API rejected request (${downloadRes.status})` },
        { status: 502 }
      );
    }

    const downloadData: any = await downloadRes.json();
    const downloadLink = downloadData.link;
    if (!downloadLink) {
      return Response.json({ error: 'OpenSubtitles returned no download link' }, { status: 502 });
    }

    // Fetch subtitle text bytes from CDN link
    const fileRes = await fetch(downloadLink);
    if (!fileRes.ok) {
      return Response.json({ error: 'Failed to download subtitle file from provider CDN' }, { status: 502 });
    }

    const subtitleBytes = await fileRes.arrayBuffer();

    return new Response(subtitleBytes, {
      headers: {
        'Content-Type': 'text/plain; charset=utf-8',
        'Cache-Control': 'public, max-age=86400',
      },
    });
  } catch (e: any) {
    return Response.json({ error: `Subtitle download error: ${e.message}` }, { status: 502 });
  }
}
