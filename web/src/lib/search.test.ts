import { expect, it, vi } from 'vitest';
import { insertRanked, normalize, scoreTitle, searchProgressively } from './search';
import type { CatalogItemSummary, SearchResponse } from './types';

const response = (id: string, anime = false): SearchResponse => ({
  items: [{ id, name: id, media_type: 'series', is_anime: anime }], notes: [],
});

it('shows fast results while another provider is pending', async () => {
  let finishAnime!: (value: SearchResponse) => void;
  const anime = new Promise<SearchResponse>(resolve => { finishAnime = resolve; });
  const update = vi.fn();
  const search = searchProgressively('fixture', new AbortController().signal, update,
    async (_query, catalog) => catalog === 'anime' ? anime : response(catalog));
  await vi.waitFor(() => expect(update).toHaveBeenCalledTimes(2));
  expect(update.mock.lastCall?.[0].items).toHaveLength(2);
  expect(update.mock.lastCall?.[1]).toEqual(['Anime']);
  finishAnime(response('series', true));
  await search;
  expect(update.mock.lastCall?.[0].items).toHaveLength(2);
  expect(update.mock.lastCall?.[0].items.find((item: { id: string }) => item.id === 'series').is_anime).toBe(true);
  expect(update.mock.lastCall?.[1]).toEqual([]);
});

it('keeps successful results when another provider fails', async () => {
  const update = vi.fn();
  await searchProgressively('fixture', new AbortController().signal, update, async (_query, catalog) => {
    if (catalog === 'anime') throw new Error('unavailable');
    return response(catalog);
  });
  expect(update.mock.lastCall?.[0].items).toHaveLength(2);
  expect(update.mock.lastCall?.[0].notes).toEqual(['Anime search failed. Try again.']);
});

it('discards all results after cancellation', async () => {
  const controller = new AbortController();
  const update = vi.fn();
  const search = searchProgressively('old query', controller.signal, update, async () => response('old'));
  controller.abort();
  await search;
  expect(update).not.toHaveBeenCalled();
});

const item = (name: string, extra: Partial<CatalogItemSummary> = {}): CatalogItemSummary => ({
  id: name,
  name,
  media_type: 'movie',
  is_anime: false,
  ...extra,
});

it('normalizes case, accents, and punctuation', () => {
  expect(normalize('  Amélie: The (Fabulous) Destiny!! ')).toBe('amelie the fabulous destiny');
  expect(normalize('Spider-Man')).toBe('spider man');
});

it('ranks an exact title above a prefix, a word prefix, and a loose match', () => {
  const names = ['Heat Wave Rising', 'The Heat', 'Heat', 'Body Heat Stories', 'Cold'];
  const ranked = [...names]
    .map((name) => ({ name, score: scoreTitle('heat', item(name)) }))
    .sort((a, b) => b.score - a.score)
    .map((entry) => entry.name);
  expect(ranked).toEqual(['Heat', 'Heat Wave Rising', 'The Heat', 'Body Heat Stories', 'Cold']);
});

it('matches a query with accents and punctuation the same as a plain one', () => {
  expect(scoreTitle('amelie', item('Amélie'))).toBe(scoreTitle('Amélie', item('Amélie')));
  expect(scoreTitle('spider man', item('Spider-Man'))).toBeGreaterThan(scoreTitle('spider man', item('Man of Spider Legends')));
});

it('gives a title with all query words a higher score than one with some words', () => {
  const both = scoreTitle('dark knight', item('The Knight of the Dark'));
  const one = scoreTitle('dark knight', item('Knight Rider'));
  expect(both).toBeGreaterThan(one);
});

it('inserts a late result by score and never moves a result already shown', () => {
  const first = insertRanked([], [item('Batman Begins'), item('Batman Forever')], 'batman');
  const before = first.map((entry) => entry.item.name);
  const next = insertRanked(first, [item('Batman'), item('Zzz Batman')], 'batman');
  const after = next.map((entry) => entry.item.name);
  expect(after[0]).toBe('Batman');
  expect(after.filter((name) => before.includes(name))).toEqual(before);
});

it('keeps results from two catalogs in ranked order as they arrive', async () => {
  const update = vi.fn();
  await searchProgressively('batman', new AbortController().signal, update, async (_query, catalog) => ({
    items: catalog === 'movie' ? [item('Batman Returns')] : catalog === 'series' ? [item('Batman')] : [],
    notes: [],
  }));
  const names = update.mock.lastCall?.[0].items.map((entry: CatalogItemSummary) => entry.name);
  expect(names).toEqual(['Batman', 'Batman Returns']);
});
