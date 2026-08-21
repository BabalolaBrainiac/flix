import type { Env, SubtitleResolveResponse } from '../types';
import { authenticateDevice, checkRateLimit } from '../auth';

const MAX_SUBTITLE_SIZE = 10 * 1024 * 1024; // 10 MiB

function isValidImdbId(id: string): boolean {
  return /^tt\d{6,10}$/.test(id) || /^\d{6,10}$/.test(id);
}

function isApprovedProviderUrl(urlStr: string): boolean {
  try {
    const parsed = new URL(urlStr);
    if (parsed.protocol !== 'https:') return false;
    const hostname = parsed.hostname.toLowerCase();
    return (
      hostname.endsWith('.opensubtitles.org') ||
      hostname.endsWith('.opensubtitles.com') ||
      hostname === 'opensubtitles.org' ||
      hostname === 'opensubtitles.com'
    );
  } catch {
    return false;
  }
}

export async function handleSubtitlesResolve(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `subresolve:${authRes.tokenHash}`, 30, 60);
  if (!rateOk) {
    return Response.json({ error: 'Subtitle resolution rate limit exceeded' }, { status: 429 });
  }

  let body: any;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: 'Invalid JSON body' }, { status: 400 });
  }

  // Validate strict request parameters
  const imdbIdRaw = typeof body.imdb_id === 'string' ? body.imdb_id.trim() : '';
  if (!imdbIdRaw || !isValidImdbId(imdbIdRaw)) {
    // An IMDb id is not a secret. Logging the rejected value shows when a
    // client sends an id the gateway cannot use, for example an episode
    // stream id like "tt0903747:1:1".
    console.log('subresolve_bad_imdb', JSON.stringify({ imdb_id: imdbIdRaw }));
    return Response.json(
      { error: 'Invalid or missing imdb_id parameter. Must be a valid IMDb ID (e.g. tt10161886).' },
      { status: 400 }
    );
  }
  console.log(
    'subresolve_request',
    JSON.stringify({ imdb_id: imdbIdRaw, season: body.season, episode: body.episode })
  );

  const season = typeof body.season === 'number' && Number.isInteger(body.season) && body.season >= 0
    ? body.season
    : undefined;
  const episode = typeof body.episode === 'number' && Number.isInteger(body.episode) && body.episode >= 0
    ? body.episode
    : undefined;

  // Reject unexpected parameters (magnets, hashes, release names, arbitrary URLs)
  const forbiddenKeys = ['magnet', 'hash', 'info_hash', 'url', 'release', 'file_id', 'query'];
  for (const key of forbiddenKeys) {
    if (key in body) {
      return Response.json(
        { error: `Forbidden parameter '${key}' provided. Only imdb_id, season, and episode are permitted.` },
        { status: 400 }
      );
    }
  }

  const apiKey = env.OPENSUBTITLES_API_KEY;
  if (!apiKey) {
    const noMatch: SubtitleResolveResponse = {
      status: 'no_match',
      reason: 'Subtitle gateway provider is not configured',
    };
    return Response.json(noMatch);
  }

  const numericImdb = imdbIdRaw.replace(/^tt/, '');
  const searchUrl = new URL('https://api.opensubtitles.com/api/v1/subtitles');
  searchUrl.searchParams.set('languages', 'en');
  searchUrl.searchParams.set('order_by', 'download_count');
  searchUrl.searchParams.set('order_direction', 'desc');
  searchUrl.searchParams.set('imdb_id', numericImdb);

  if (season !== undefined) {
    searchUrl.searchParams.set('season_number', season.toString());
  }
  if (episode !== undefined) {
    searchUrl.searchParams.set('episode_number', episode.toString());
  }

  try {
    const userAgent = env.OPENSUBTITLES_USER_AGENT || 'Flix Gateway v0.1.0';
    const searchRes = await fetch(searchUrl.toString(), {
      headers: {
        'Api-Key': apiKey,
        'User-Agent': userAgent,
        Accept: 'application/json',
      },
    });

    if (!searchRes.ok) {
      console.log('subresolve_search_failed', JSON.stringify({ status: searchRes.status }));
      const noMatch: SubtitleResolveResponse = {
        status: 'no_match',
        reason: 'Provider search returned no verified matches',
      };
      return Response.json(noMatch);
    }

    const searchData: any = await searchRes.json();
    if (!Array.isArray(searchData.data) || searchData.data.length === 0) {
      const noMatch: SubtitleResolveResponse = {
        status: 'no_match',
        reason: 'No English subtitles found for media',
      };
      return Response.json(noMatch);
    }

    // Find best candidate with valid file_id
    let bestCandidate: { file_id: number; file_name?: string } | null = null;
    for (const item of searchData.data) {
      const file = item.attributes?.files?.[0];
      if (file?.file_id && typeof file.file_id === 'number') {
        bestCandidate = {
          file_id: file.file_id,
          file_name: item.attributes?.release || file.file_name,
        };
        break;
      }
    }

    if (!bestCandidate) {
      const noMatch: SubtitleResolveResponse = {
        status: 'no_match',
        reason: 'No downloadable English subtitle files in provider results',
      };
      return Response.json(noMatch);
    }

    // Acquire session token from Durable Object
    const doId = env.OPENSUBTITLES_COORDINATOR.idFromName('global_coordinator');
    const coordinator: any = env.OPENSUBTITLES_COORDINATOR.get(doId);
    let sessionToken: string;
    if (typeof coordinator.getSession === 'function') {
      const session = await coordinator.getSession();
      sessionToken = session.token;
    } else {
      const tokenRes = await coordinator.fetch('https://coordinator/token', { method: 'POST' });
      if (!tokenRes.ok) {
        return Response.json(
          { error: 'Failed to acquire subtitle session' },
          { status: 502 }
        );
      }
      const tokenData: any = await tokenRes.json();
      sessionToken = tokenData.token;
    }

    // Request download link
    const downloadRes = await fetch('https://api.opensubtitles.com/api/v1/download', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Api-Key': apiKey,
        Authorization: `Bearer ${sessionToken}`,
        'User-Agent': userAgent,
        Accept: 'application/json',
      },
      body: JSON.stringify({ file_id: bestCandidate.file_id }),
    });

    if (!downloadRes.ok) {
      const noMatch: SubtitleResolveResponse = {
        status: 'no_match',
        reason: 'Provider rejected subtitle download request',
      };
      return Response.json(noMatch);
    }

    const downloadData: any = await downloadRes.json();
    const downloadLink = downloadData.link;
    if (!downloadLink || !isApprovedProviderUrl(downloadLink)) {
      return Response.json(
        { error: 'Provider returned an unapproved or invalid download link' },
        { status: 502 }
      );
    }

    const fileRes = await fetch(downloadLink);
    if (!fileRes.ok) {
      return Response.json(
        { error: 'Failed to retrieve subtitle file from provider' },
        { status: 502 }
      );
    }

    const contentLength = Number(fileRes.headers.get('content-length') || 0);
    if (contentLength > MAX_SUBTITLE_SIZE) {
      return Response.json(
        { error: 'Subtitle file exceeded size limit' },
        { status: 502 }
      );
    }

    const textContent = await fileRes.text();
    if (textContent.length > MAX_SUBTITLE_SIZE) {
      return Response.json(
        { error: 'Subtitle file exceeded size limit' },
        { status: 502 }
      );
    }

    const matchedResponse: SubtitleResolveResponse = {
      status: 'matched',
      file_id: bestCandidate.file_id,
      file_name: bestCandidate.file_name,
      content: textContent,
      format: 'srt',
    };

    return Response.json(matchedResponse);
  } catch (err: any) {
    // Log the message only. A stack can carry a signed provider download URL,
    // which is a short-lived credential and must not enter the logs.
    console.error(
      'subresolve_error',
      JSON.stringify({ message: err && err.message ? String(err.message) : 'unknown' })
    );
    return Response.json(
      { error: 'Subtitle resolution encountered an unexpected error' },
      { status: 502 }
    );
  }
}
