import type { CatalogSearchItem, Env } from '../types';
import { authenticateDevice, checkRateLimit } from '../auth';

const CINEMETA_BASE = 'https://v3-cinemeta.strem.io';
const ANIME_KITSU_BASE = 'https://anime-kitsu.strem.fun';

export async function handleCatalogSearch(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const rateOk = await checkRateLimit(env, `search:${authRes.tokenHash}`, 60, 60);
  if (!rateOk) {
    return Response.json({ error: 'Search rate limit exceeded. Try again in a minute.' }, { status: 429 });
  }

  const url = new URL(request.url);
  const query = (url.searchParams.get('q') || '').trim();
  const animeOnly = url.searchParams.get('anime_only') === 'true';

  if (!query) {
    return Response.json({ error: 'Missing query parameter q' }, { status: 400 });
  }

  const encodedQuery = encodeURIComponent(query);
  const items: CatalogSearchItem[] = [];
  const notes: string[] = [];

  try {
    if (animeOnly) {
      const animeUrl = `${ANIME_KITSU_BASE}/catalog/anime/kitsu-anime-list/search=${encodedQuery}.json`;
      const res = await fetch(animeUrl, { headers: { Accept: 'application/json' } });
      if (res.ok) {
        const data: any = await res.json();
        if (Array.isArray(data.metas)) {
          for (const item of data.metas) {
            items.push({
              id: item.id,
              name: sanitizeText(item.name),
              media_type: 'anime',
              release_info: item.releaseInfo,
              is_anime: true,
            });
          }
        }
      }
    } else {
      const movieUrl = `${CINEMETA_BASE}/catalog/movie/top/search=${encodedQuery}.json`;
      const seriesUrl = `${CINEMETA_BASE}/catalog/series/top/search=${encodedQuery}.json`;
      const animeUrl = `${ANIME_KITSU_BASE}/catalog/anime/kitsu-anime-list/search=${encodedQuery}.json`;

      const [movieRes, seriesRes, animeRes] = await Promise.all([
        fetch(movieUrl, { headers: { Accept: 'application/json' } }).catch(() => null),
        fetch(seriesUrl, { headers: { Accept: 'application/json' } }).catch(() => null),
        fetch(animeUrl, { headers: { Accept: 'application/json' } }).catch(() => null),
      ]);

      if (movieRes && movieRes.ok) {
        const data: any = await movieRes.json();
        if (Array.isArray(data.metas)) {
          for (const item of data.metas) {
            items.push({
              id: item.id,
              name: sanitizeText(item.name),
              media_type: 'movie',
              release_info: item.releaseInfo,
              is_anime: false,
            });
          }
        }
      }

      if (seriesRes && seriesRes.ok) {
        const data: any = await seriesRes.json();
        if (Array.isArray(data.metas)) {
          for (const item of data.metas) {
            items.push({
              id: item.id,
              name: sanitizeText(item.name),
              media_type: 'series',
              release_info: item.releaseInfo,
              is_anime: false,
            });
          }
        }
      }

      if (animeRes && animeRes.ok) {
        const data: any = await animeRes.json();
        if (Array.isArray(data.metas)) {
          for (const item of data.metas) {
            items.push({
              id: item.id,
              name: sanitizeText(item.name),
              media_type: 'anime',
              release_info: item.releaseInfo,
              is_anime: true,
            });
          }
        }
      }
    }
  } catch (e: any) {
    notes.push(`Catalog fetch note: ${e.message}`);
  }

  return Response.json({ items, notes });
}

export async function handleCatalogTitle(request: Request, env: Env): Promise<Response> {
  const authRes = await authenticateDevice(request, env);
  if (authRes instanceof Response) return authRes;

  const url = new URL(request.url);
  const id = (url.searchParams.get('id') || '').trim();
  const isAnime = url.searchParams.get('is_anime') === 'true';

  if (!id || !/^[a-zA-Z0-9:_-]+$/.test(id)) {
    return Response.json({ error: 'Invalid or missing title id' }, { status: 400 });
  }

  const base = isAnime ? ANIME_KITSU_BASE : CINEMETA_BASE;
  const mediaType = isAnime ? 'series' : 'series';
  const metaUrl = `${base}/meta/${mediaType}/${encodeURIComponent(id)}.json`;

  try {
    const res = await fetch(metaUrl, { headers: { Accept: 'application/json' } });
    if (!res.ok) {
      return Response.json({ error: `Upstream metadata error (${res.status})` }, { status: 502 });
    }
    const data: any = await res.json();
    const videos = Array.isArray(data?.meta?.videos) ? data.meta.videos : [];
    const episodes = videos.map((v: any) => ({
      id: v.id,
      title: sanitizeText(v.title || ''),
      season: v.season || 1,
      episode: v.episode || 1,
    }));

    return Response.json({ episodes });
  } catch (e: any) {
    return Response.json({ error: `Catalog title fetch failed: ${e.message}` }, { status: 502 });
  }
}

function sanitizeText(value: string): string {
  if (!value) return '';
  return value.replace(/[\x00-\x1F\x7F]/g, '').trim().substring(0, 200);
}
