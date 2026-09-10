import { searchCatalog } from './api';
import type { CatalogItemSummary, SearchResponse } from './types';

export async function searchProgressively(
  query: string,
  signal: AbortSignal,
  update: (response: SearchResponse, pending: string[]) => void,
  fetchCatalog = searchCatalog,
): Promise<void> {
  const catalogs = ['movie', 'series', 'anime'] as const;
  const labels = { movie: 'Movies', series: 'Series', anime: 'Anime' };
  const pending = new Set<string>(catalogs.map(catalog => labels[catalog]));
  const items = new Map<string, CatalogItemSummary>();
  const notes: string[] = [];
  await Promise.all(catalogs.map(async catalog => {
    try {
      const response = await fetchCatalog(query, catalog, signal);
      if (signal.aborted) return;
      for (const item of response.items) {
        const previous = items.get(item.id);
        items.set(item.id, { ...item, is_anime: item.is_anime || !!previous?.is_anime });
      }
      notes.push(...response.notes);
    } catch {
      if (signal.aborted) return;
      notes.push(`${labels[catalog]} search failed. Try again.`);
    }
    pending.delete(labels[catalog]);
    update({ items: [...items.values()], notes: [...new Set(notes)] }, [...pending]);
  }));
}
