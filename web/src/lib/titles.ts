import { navigate } from './router';
import type { CatalogItemSummary } from './types';

const STORAGE_PREFIX = 'flix_title:';
const known = new Map<string, CatalogItemSummary>();

/** Keeps a title that the user opened, so its page can load again after a reload. */
export function rememberTitle(item: CatalogItemSummary) {
  known.set(item.id, item);
  try {
    sessionStorage.setItem(STORAGE_PREFIX + item.id, JSON.stringify(item));
  } catch {
    // Storage can be blocked. The title still opens from memory.
  }
}

export function recallTitle(id: string): CatalogItemSummary | null {
  const inMemory = known.get(id);
  if (inMemory) return inMemory;
  try {
    const raw = sessionStorage.getItem(STORAGE_PREFIX + id);
    return raw ? (JSON.parse(raw) as CatalogItemSummary) : null;
  } catch {
    return null;
  }
}

/** Opens the page of a title. Back returns to the list that the user came from. */
export function openTitle(item: CatalogItemSummary) {
  rememberTitle(item);
  navigate({ name: 'title', titleId: item.id });
}

interface StoredTitle {
  catalog_id: string;
  title?: string | null;
  media_type?: string | null;
  poster?: string | null;
  year?: number | null;
}

/**
 * Turns a stored list item or status into the shape that the title page
 * needs. A pre-media_type row (schema v1) has no media_type - default to
 * `movie` rather than leaving the title page unable to load anything.
 */
export function catalogItemOf(stored: StoredTitle): CatalogItemSummary {
  return {
    id: stored.catalog_id,
    name: stored.title || stored.catalog_id,
    media_type: stored.media_type || 'movie',
    release_info: stored.year ? String(stored.year) : undefined,
    poster: stored.poster ?? undefined,
    is_anime: stored.catalog_id.startsWith('kitsu:'),
  };
}
