import { get, writable } from 'svelte/store';
import * as api from './api';
import type { ListItemSummary, ListSummary, TitleRef, WatchStatusSummary } from './types';

export type { TitleRef };

interface CatalogLike {
  id: string;
  name: string;
  media_type: string;
  release_info?: string | null;
  poster?: string | null;
}

export function yearOf(releaseInfo?: string | null): number | null {
  const match = releaseInfo?.match(/\d{4}/);
  return match ? Number(match[0]) : null;
}

export function titleRefOf(item: CatalogLike, canonicalId?: string | null): TitleRef {
  return {
    catalogId: item.id,
    mediaType: item.media_type,
    title: item.name,
    poster: item.poster ?? null,
    year: yearOf(item.release_info),
    canonicalId: canonicalId ?? null,
  };
}

/** The IMDb id inside a stream or episode id such as `tt7124066:1:7`. */
export function imdbIdOf(value?: string | null): string | null {
  return value?.match(/tt\d{5,}/)?.[0] ?? null;
}

/** Formats the season and episode stored at the end of an episode id. */
export function episodeLabel(episodeId?: string | null): string | null {
  if (!episodeId) return null;
  const parts = episodeId.split(':');
  if (parts.length < 3) return null;
  const season = Number(parts.at(-2));
  const episode = Number(parts.at(-1));
  if (!Number.isInteger(season) || season < 0 || !Number.isInteger(episode) || episode < 0) return null;
  return `S${String(season).padStart(2, '0')}E${String(episode).padStart(2, '0')}`;
}

export interface Membership {
  profileId: string | null;
  lists: ListSummary[];
  statuses: WatchStatusSummary[];
  loaded: boolean;
}

export const EMPTY_MEMBERSHIP: Membership = { profileId: null, lists: [], statuses: [], loaded: false };
export const membership = writable<Membership>(EMPTY_MEMBERSHIP);

// ---- Matching. One title can carry two ids. ----

function keysOf(id: string, canonical?: string | null): Set<string> {
  return new Set(canonical ? [id, canonical] : [id]);
}

function sharesKey(a: Set<string>, b: Set<string>): boolean {
  for (const key of a) if (b.has(key)) return true;
  return false;
}

function matches(ids: Set<string>, entry: { catalog_id: string; canonical_id?: string | null }): boolean {
  return sharesKey(ids, keysOf(entry.catalog_id, entry.canonical_id));
}

export function idsOf(ref: Pick<TitleRef, 'catalogId' | 'canonicalId'>): Set<string> {
  return keysOf(ref.catalogId, ref.canonicalId);
}

/** The status of a title, or `null` when it has none. */
export function findStatus(m: Membership, ids: Set<string>): WatchStatusSummary | null {
  return m.statuses.find((entry) => matches(ids, entry)) ?? null;
}

/** The lists that hold a title. */
export function listsContaining(m: Membership, ids: Set<string>): ListSummary[] {
  return m.lists.filter((list) => list.items.some((item) => matches(ids, item)));
}

export function listNameTaken(m: Membership, name: string): boolean {
  const wanted = name.trim().toLowerCase();
  return m.lists.some((list) => list.name.trim().toLowerCase() === wanted);
}

// ---- Pure reducers. Each returns a new Membership. ----

export function withStatus(m: Membership, ref: TitleRef, status: string, now: number): Membership {
  const ids = idsOf(ref);
  const previous = findStatus(m, ids);
  const entry: WatchStatusSummary = {
    catalog_id: ref.catalogId,
    status,
    episode_id: previous?.episode_id ?? null,
    resume_seconds: previous?.resume_seconds ?? null,
    updated_at: now,
    title: ref.title ?? previous?.title ?? null,
    poster: ref.poster ?? previous?.poster ?? null,
    media_type: ref.mediaType,
    year: ref.year ?? previous?.year ?? null,
    canonical_id: ref.canonicalId ?? previous?.canonical_id ?? null,
  };
  // One status for each title: the new one replaces every entry for the same title.
  const others = m.statuses.filter((existing) => !matches(ids, existing));
  return { ...m, statuses: [entry, ...others] };
}

export function withoutStatus(m: Membership, ids: Set<string>): Membership {
  return { ...m, statuses: m.statuses.filter((entry) => !matches(ids, entry)) };
}

export function withListItem(m: Membership, listId: string, ref: TitleRef, now: number): Membership {
  const ids = idsOf(ref);
  const item: ListItemSummary = {
    catalog_id: ref.catalogId,
    media_type: ref.mediaType,
    added_at: now,
    title: ref.title ?? null,
    poster: ref.poster ?? null,
    year: ref.year ?? null,
    canonical_id: ref.canonicalId ?? null,
  };
  const lists = m.lists.map((list) =>
    list.id !== listId || list.items.some((existing) => matches(ids, existing))
      ? list
      : { ...list, items: [...list.items, item] },
  );
  return { ...m, lists };
}

export function withoutListItem(m: Membership, listId: string, ids: Set<string>): Membership {
  const lists = m.lists.map((list) =>
    list.id === listId ? { ...list, items: list.items.filter((item) => !matches(ids, item)) } : list,
  );
  return { ...m, lists };
}

// ---- Loading and changes that reach the server ----

let loadRequest = 0;

/** Loads the lists and statuses of one profile. A late reply from an older call is dropped. */
export async function refreshMembership(profileId: string | null): Promise<void> {
  const request = ++loadRequest;
  if (!profileId) {
    membership.set({ ...EMPTY_MEMBERSHIP, loaded: true });
    return;
  }
  try {
    const [lists, statuses] = await Promise.all([api.getLists(profileId), api.getWatchStatus(profileId)]);
    if (request !== loadRequest) return;
    membership.set({ profileId, lists: lists.lists, statuses: statuses.watch_status, loaded: true });
  } catch {
    if (request !== loadRequest) return;
    membership.update((m) => ({ ...m, profileId, loaded: true }));
  }
}

/** Shows a change at once, sends it, and restores the old state when the send fails. */
async function optimistic(apply: (m: Membership) => Membership, send: () => Promise<unknown>): Promise<void> {
  const before = get(membership);
  membership.set(apply(before));
  try {
    await send();
  } catch (error) {
    membership.set(before);
    throw error;
  }
}

export function setStatus(profileId: string, ref: TitleRef, status: string): Promise<void> {
  return optimistic(
    (m) => withStatus(m, ref, status, Date.now()),
    () => api.setWatchStatus(profileId, ref, status),
  );
}

/** Removes the status of a title. The server removes the stored id, so the call uses it. */
export async function clearStatus(profileId: string, ids: Set<string>): Promise<void> {
  const stored = findStatus(get(membership), ids);
  if (!stored) return;
  await optimistic(
    (m) => withoutStatus(m, ids),
    () => api.removeWatchStatus(profileId, stored.catalog_id),
  );
}

export function addToList(listId: string, ref: TitleRef): Promise<void> {
  return optimistic(
    (m) => withListItem(m, listId, ref, Date.now()),
    () => api.addListItem(listId, ref),
  );
}

export async function removeFromList(listId: string, ids: Set<string>): Promise<void> {
  const list = get(membership).lists.find((candidate) => candidate.id === listId);
  const stored = list?.items.find((item) => matches(ids, item));
  if (!stored) return;
  await optimistic(
    (m) => withoutListItem(m, listId, ids),
    () => api.removeListItem(listId, stored.catalog_id),
  );
}

/** Creates a list. The caller checks the name first, and the server checks it again. */
export async function createList(profileId: string, name: string): Promise<ListSummary | null> {
  await api.createList(profileId, name.trim());
  await refreshMembership(profileId);
  const wanted = name.trim().toLowerCase();
  return get(membership).lists.find((list) => list.name.toLowerCase() === wanted) ?? null;
}

export async function deleteList(profileId: string, listId: string): Promise<void> {
  await api.deleteList(listId);
  await refreshMembership(profileId);
}

// ---- Badges: one lookup table for every card on screen ----

export interface Badge {
  status: string | null;
  lists: { id: string; name: string }[];
}

export type BadgeIndex = Map<string, Badge>;

function badgeAt(index: BadgeIndex, key: string): Badge {
  let badge = index.get(key);
  if (!badge) {
    badge = { status: null, lists: [] };
    index.set(key, badge);
  }
  return badge;
}

/** Builds the table once for each change of the membership. A card then reads it with no request. */
export function buildBadgeIndex(m: Membership): BadgeIndex {
  const index: BadgeIndex = new Map();
  for (const entry of m.statuses) {
    for (const key of keysOf(entry.catalog_id, entry.canonical_id)) badgeAt(index, key).status = entry.status;
  }
  for (const list of m.lists) {
    for (const item of list.items) {
      for (const key of keysOf(item.catalog_id, item.canonical_id)) {
        const badge = badgeAt(index, key);
        if (!badge.lists.some((known) => known.id === list.id)) badge.lists.push({ id: list.id, name: list.name });
      }
    }
  }
  return index;
}

export function badgeFor(index: BadgeIndex, id: string, canonicalId?: string | null): Badge | null {
  const badge = index.get(id) ?? (canonicalId ? index.get(canonicalId) : undefined);
  return badge && (badge.status || badge.lists.length > 0) ? badge : null;
}

export const STATUS_LABELS: Record<string, string> = {
  to_watch: 'To watch',
  in_progress: 'In progress',
  watched: 'Watched',
};

// ---- Backfill: fill in a title stored before this app captured titles ----

const backfillInFlight = new Set<string>();

function patchTitleData(ids: Set<string>, ref: TitleRef) {
  membership.update((m) => ({
    ...m,
    lists: m.lists.map((list) => ({
      ...list,
      items: list.items.map((item) =>
        matches(ids, item) && !item.title
          ? { ...item, title: ref.title, poster: ref.poster, year: ref.year }
          : item,
      ),
    })),
    statuses: m.statuses.map((entry) =>
      matches(ids, entry) && !entry.title
        ? { ...entry, title: ref.title, poster: ref.poster, year: ref.year }
        : entry,
    ),
  }));
}

/**
 * Looks up and fills in the title of a list item or a status that has none
 * - from a device that added it before this app captured titles. Shows the
 * result at once and persists it quietly, through the normal add/status
 * calls, so it stays fixed after this session.
 */
export async function backfillTitleIfMissing(
  profileId: string | null,
  catalogId: string,
  mediaType: string,
): Promise<void> {
  if (backfillInFlight.has(catalogId)) return;
  backfillInFlight.add(catalogId);
  try {
    const meta = await api.getTitleMeta(catalogId, mediaType);
    if (!meta.name) return;
    const ref: TitleRef = {
      catalogId,
      mediaType,
      title: meta.name,
      poster: meta.poster ?? null,
      year: yearOf(meta.release_info),
    };
    const ids = idsOf(ref);
    patchTitleData(ids, ref);

    const current = get(membership);
    for (const list of listsContaining(current, ids)) {
      api.addListItem(list.id, ref).catch(() => {});
    }
    const status = profileId ? findStatus(current, ids) : null;
    if (profileId && status) {
      api
        .setWatchStatus(profileId, ref, status.status, status.episode_id ?? undefined, status.resume_seconds ?? undefined)
        .catch(() => {});
    }
  } catch {
    // No metadata available (removed title, network error). The raw id keeps showing.
  } finally {
    backfillInFlight.delete(catalogId);
  }
}
