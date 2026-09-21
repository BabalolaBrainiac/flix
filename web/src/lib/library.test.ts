import { describe, expect, it } from 'vitest';
import {
  EMPTY_MEMBERSHIP,
  findStatus,
  idsOf,
  imdbIdOf,
  listNameTaken,
  listsContaining,
  episodeLabel,
  titleRefOf,
  withListItem,
  withoutListItem,
  withoutStatus,
  withStatus,
  yearOf,
  type Membership,
  type TitleRef,
} from './library';

const heat: TitleRef = { catalogId: 'tt1', mediaType: 'movie', title: 'Heat', year: 1995 };
const base: Membership = {
  ...EMPTY_MEMBERSHIP,
  loaded: true,
  lists: [
    { id: 'l1', profile_id: 'p', name: 'Weekend', created_at: 1, items: [] },
    { id: 'l2', profile_id: 'p', name: 'Classics', created_at: 2, items: [] },
  ],
};

describe('title helpers', () => {
  it('reads the year from release info', () => {
    expect(yearOf('1995')).toBe(1995);
    expect(yearOf('2011–2021')).toBe(2011);
    expect(yearOf('TBA')).toBeNull();
    expect(yearOf(undefined)).toBeNull();
  });

  it('builds a title reference from a catalog item', () => {
    const ref = titleRefOf({ id: 'tt9', name: 'Show', media_type: 'series', release_info: '2020-', poster: 'p.jpg' });
    expect(ref).toMatchObject({ catalogId: 'tt9', title: 'Show', year: 2020, poster: 'p.jpg' });
  });

  it('finds the IMDb id inside an episode id', () => {
    expect(imdbIdOf('tt7124066:1:7')).toBe('tt7124066');
    expect(imdbIdOf('kitsu:13274')).toBeNull();
    expect(imdbIdOf(null)).toBeNull();
  });

  it('formats the last played episode from series and anime ids', () => {
    expect(episodeLabel('tt12345:1:4')).toBe('S01E04');
    expect(episodeLabel('kitsu:13274:2:11')).toBe('S02E11');
    expect(episodeLabel('tt12345')).toBeNull();
    expect(episodeLabel(null)).toBeNull();
  });
});

describe('status', () => {
  it('sets a status and keeps one entry for the title', () => {
    let m = withStatus(base, heat, 'to_watch', 10);
    m = withStatus(m, heat, 'in_progress', 20);
    m = withStatus(m, heat, 'watched', 30);
    expect(m.statuses).toHaveLength(1);
    expect(findStatus(m, idsOf(heat))?.status).toBe('watched');
  });

  it('replaces a status stored under an alias id of the same title', () => {
    const alias: TitleRef = { catalogId: 'kitsu:7', mediaType: 'anime', canonicalId: 'tt99' };
    const canonical: TitleRef = { catalogId: 'tt99', mediaType: 'series' };
    let m = withStatus(base, alias, 'to_watch', 10);
    m = withStatus(m, canonical, 'watched', 20);
    expect(m.statuses.map((s) => s.catalog_id)).toEqual(['tt99']);
    expect(findStatus(m, idsOf(alias))?.status).toBe('watched');
  });

  it('keeps stored data when a later call has none', () => {
    let m = withStatus(base, heat, 'to_watch', 10);
    m = withStatus(m, { catalogId: 'tt1', mediaType: 'movie' }, 'watched', 20);
    expect(m.statuses[0]).toMatchObject({ title: 'Heat', year: 1995, status: 'watched' });
  });

  it('removes a status and leaves other titles alone', () => {
    let m = withStatus(base, heat, 'watched', 10);
    m = withStatus(m, { catalogId: 'tt2', mediaType: 'movie' }, 'to_watch', 11);
    m = withoutStatus(m, idsOf(heat));
    expect(m.statuses.map((s) => s.catalog_id)).toEqual(['tt2']);
  });

  it('does not change the old state object', () => {
    const before = withStatus(base, heat, 'watched', 10);
    const after = withoutStatus(before, idsOf(heat));
    expect(before.statuses).toHaveLength(1);
    expect(after.statuses).toHaveLength(0);
  });
});

describe('lists', () => {
  it('adds a title to one list only, and never twice', () => {
    let m = withListItem(base, 'l1', heat, 10);
    m = withListItem(m, 'l1', heat, 20);
    expect(m.lists[0].items).toHaveLength(1);
    expect(m.lists[1].items).toHaveLength(0);
    expect(listsContaining(m, idsOf(heat)).map((l) => l.id)).toEqual(['l1']);
  });

  it('does not add an alias id of a title that the list already holds', () => {
    let m = withListItem(base, 'l1', { catalogId: 'tt99', mediaType: 'series' }, 10);
    m = withListItem(m, 'l1', { catalogId: 'kitsu:7', mediaType: 'anime', canonicalId: 'tt99' }, 20);
    expect(m.lists[0].items).toHaveLength(1);
  });

  it('removes a title from one list', () => {
    let m = withListItem(base, 'l1', heat, 10);
    m = withListItem(m, 'l2', heat, 10);
    m = withoutListItem(m, 'l1', idsOf(heat));
    expect(listsContaining(m, idsOf(heat)).map((l) => l.id)).toEqual(['l2']);
  });

  it('finds a taken list name in any letter case', () => {
    expect(listNameTaken(base, 'weekend')).toBe(true);
    expect(listNameTaken(base, '  CLASSICS ')).toBe(true);
    expect(listNameTaken(base, 'New list')).toBe(false);
  });
});

import { badgeFor, buildBadgeIndex } from './library';

describe('badges', () => {
  it('reads status and lists for a title from one table', () => {
    let m = withStatus(base, heat, 'in_progress', 10);
    m = withListItem(m, 'l1', heat, 10);
    m = withListItem(m, 'l2', heat, 10);
    const badge = badgeFor(buildBadgeIndex(m), 'tt1');
    expect(badge?.status).toBe('in_progress');
    expect(badge?.lists.map((l) => l.name)).toEqual(['Weekend', 'Classics']);
  });

  it('gives no badge to a title with no status and no list', () => {
    expect(badgeFor(buildBadgeIndex(base), 'tt404')).toBeNull();
  });

  it('matches a title through its alias id', () => {
    const m = withStatus(base, { catalogId: 'kitsu:7', mediaType: 'anime', canonicalId: 'tt99' }, 'watched', 10);
    const index = buildBadgeIndex(m);
    expect(badgeFor(index, 'tt99')?.status).toBe('watched');
    expect(badgeFor(index, 'kitsu:7')?.status).toBe('watched');
  });
});
