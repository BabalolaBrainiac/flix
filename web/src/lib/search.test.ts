import { expect, it, vi } from 'vitest';
import { searchProgressively } from './search';
import type { SearchResponse } from './types';

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
