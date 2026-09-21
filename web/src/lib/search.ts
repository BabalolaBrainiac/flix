import { searchCatalog } from './api';
import type { CatalogItemSummary, SearchResponse } from './types';

/** Lowercases, removes accents and punctuation, and joins words with one space. */
export function normalize(text: string): string {
  return text
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, ' ')
    .trim();
}

// Score bands. A higher band always beats a lower band. The small adjustments
// below only order titles inside one band.
const EXACT = 1000;
const PREFIX = 800;
const WORD_PREFIX = 700;
const ALL_WORDS = 600;
const ALL_SUBSTRINGS = 400;
const PARTIAL = 300;

function bandFor(title: string, query: string, tokens: string[]): number {
  if (title === query) return EXACT;
  if (title.startsWith(query)) return PREFIX;
  const words = title.split(' ');
  if (words.some((word) => word.startsWith(query))) return WORD_PREFIX;
  if (tokens.every((token) => words.includes(token))) return ALL_WORDS;
  if (tokens.every((token) => title.includes(token))) return ALL_SUBSTRINGS;
  const found = tokens.filter((token) => title.includes(token)).length;
  return Math.round((PARTIAL * found) / tokens.length);
}

/** How well a result fits the query. A higher number ranks first. */
export function scoreTitle(query: string, item: CatalogItemSummary): number {
  const q = normalize(query);
  const title = normalize(item.name);
  if (!q || !title) return 0;
  const band = bandFor(title, q, q.split(' '));
  // A title close to the query in length reads as a better match.
  const lengthPenalty = Math.min(30, Math.abs(title.length - q.length) / 2);
  const detail = (item.release_info ? 5 : 0) + (item.poster ? 3 : 0);
  return band - lengthPenalty + detail;
}

interface Ranked {
  item: CatalogItemSummary;
  score: number;
}

/**
 * Adds new results to a ranked list. Each new result goes before the first
 * result with a lower score. Results already in the list keep their order, so
 * a card that the user sees never moves when a slow catalog answers later.
 */
export function insertRanked(list: Ranked[], incoming: CatalogItemSummary[], query: string): Ranked[] {
  const next = [...list];
  for (const item of incoming) {
    const score = scoreTitle(query, item);
    const at = next.findIndex((existing) => existing.score < score);
    next.splice(at === -1 ? next.length : at, 0, { item, score });
  }
  return next;
}

export async function searchProgressively(
  query: string,
  signal: AbortSignal,
  update: (response: SearchResponse, pending: string[]) => void,
  fetchCatalog = searchCatalog,
): Promise<void> {
  const catalogs = ['movie', 'series', 'anime'] as const;
  const labels = { movie: 'Movies', series: 'Series', anime: 'Anime' };
  const pending = new Set<string>(catalogs.map(catalog => labels[catalog]));
  let ranked: Ranked[] = [];
  const known = new Map<string, CatalogItemSummary>();
  const notes: string[] = [];

  // A title that two catalogs both return keeps its first place. The anime
  // flag is kept from either copy.
  function absorb(items: CatalogItemSummary[]): CatalogItemSummary[] {
    const fresh: CatalogItemSummary[] = [];
    for (const item of items) {
      const previous = known.get(item.id);
      if (previous) {
        if (item.is_anime && !previous.is_anime) previous.is_anime = true;
      } else {
        const copy = { ...item };
        known.set(item.id, copy);
        fresh.push(copy);
      }
    }
    return fresh;
  }

  await Promise.all(catalogs.map(async catalog => {
    try {
      const response = await fetchCatalog(query, catalog, signal);
      if (signal.aborted) return;
      ranked = insertRanked(ranked, absorb(response.items), query);
      notes.push(...response.notes);
    } catch {
      if (signal.aborted) return;
      notes.push(`${labels[catalog]} search failed. Try again.`);
    }
    pending.delete(labels[catalog]);
    update({ items: ranked.map(entry => entry.item), notes: [...new Set(notes)] }, [...pending]);
  }));
}
