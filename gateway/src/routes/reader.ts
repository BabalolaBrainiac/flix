import { fetchMetadata } from '../upstream';
import type { Env } from '../types';
import { authenticateDevice, checkRateLimit } from '../auth';

const MANGADEX_BASE = 'https://api.mangadex.org';

export async function handleReaderSearch(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `reader:${authRes.tokenHash}`, 20, 60);
  if (!rateOk) {
    return Response.json({ error: 'Reader rate limit exceeded. Try again in a minute.' }, { status: 429 });
  }

  const url = new URL(request.url);
  const query = (url.searchParams.get('q') || '').trim();
  if (!query || query.length > 200) {
    return Response.json({ error: 'Missing query parameter q' }, { status: 400 });
  }

  const limit = Number(url.searchParams.get('limit') || '20');
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 50) {
    return Response.json({ error: 'Invalid search limit' }, { status: 400 });
  }

  try {
    const mangaUrl = `${MANGADEX_BASE}/manga?title=${encodeURIComponent(query)}&limit=${limit}&includes[]=cover_art`;
    const res = await fetchMetadata(mangaUrl, {
      headers: {
        Accept: 'application/json',
        'User-Agent': 'flix-gateway/0.1.0',
      },
    });

    if (!res.ok) {
      return Response.json({ error: `MangaDex returned ${res.status}` }, { status: 502 });
    }

    const data: any = await res.json();
    const publications = (data.data || []).map((manga: any) => {
      const attrs = manga.attributes || {};
      const title = attrs.title?.en || Object.values(attrs.title || {})[0] || '';
      const description = attrs.description?.en || Object.values(attrs.description || {})[0] || null;
      const coverRel = (manga.relationships || []).find((r: any) => r.type === 'cover_art');
      const coverFileName = coverRel?.attributes?.fileName;

      return {
        id: manga.id,
        title: sanitizeText(String(title)),
        description: description ? sanitizeText(String(description)) : null,
        year: attrs.year || null,
        status: attrs.status || null,
        tags: (attrs.tags || []).map((t: any) => t.attributes?.name?.en || '').filter(Boolean),
        cover_url: coverFileName
          ? `https://uploads.mangadex.org/covers/${manga.id}/${coverFileName}`
          : null,
        source: 'MangaDex',
      };
    });

    return Response.json({ publications });
  } catch {
    return Response.json({ error: 'Manga search is unavailable.' }, { status: 502 });
  }
}

export async function handleReaderChapters(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `reader:${authRes.tokenHash}`, 20, 60);
  if (!rateOk) {
    return Response.json({ error: 'Reader rate limit exceeded. Try again in a minute.' }, { status: 429 });
  }

  const url = new URL(request.url);
  const mangaId = (url.searchParams.get('manga_id') || '').trim();
  const lang = url.searchParams.get('lang') || 'en';

  if (!mangaId || !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(mangaId)) {
    return Response.json({ error: 'Invalid manga_id' }, { status: 400 });
  }

  try {
    const feedUrl = `${MANGADEX_BASE}/manga/${mangaId}/feed?limit=500&order[volume]=asc&order[chapter]=asc`;
    const res = await fetchMetadata(feedUrl, {
      headers: {
        Accept: 'application/json',
        'User-Agent': 'flix-gateway/0.1.0',
      },
    });

    if (!res.ok) {
      return Response.json({ error: `MangaDex returned ${res.status}` }, { status: 502 });
    }

    const data: any = await res.json();
    const allChapters = data.data || [];

    const languages = new Set<string>();
    for (const ch of allChapters) {
      const lang = ch.attributes?.translatedLanguage;
      if (lang) languages.add(lang);
    }

    const chapters = allChapters
      .filter((ch: any) => !ch.attributes?.externalUrl)
      .filter((ch: any) => ch.attributes?.translatedLanguage === lang)
      .map((ch: any) => ({
        id: ch.id,
        chapter: ch.attributes?.chapter || null,
        volume: ch.attributes?.volume || null,
        title: ch.attributes?.title ? sanitizeText(String(ch.attributes.title)) : null,
        language: ch.attributes?.translatedLanguage || '',
        pages: ch.attributes?.pages || 0,
        external_url: null,
      }));

    return Response.json({
      chapters,
      available_languages: Array.from(languages).sort(),
    });
  } catch {
    return Response.json({ error: 'Chapter metadata is unavailable.' }, { status: 502 });
  }
}

function sanitizeText(value: string): string {
  if (!value) return '';
  return value.replace(/[\x00-\x1F\x7F]/g, '').trim().substring(0, 300);
}
