import { describe, expect, it } from 'vitest';
import { formatPath, parsePath, sameRoute } from './router';

describe('parsePath', () => {
  it('maps each primary path to its route', () => {
    expect(parsePath('/discover')).toEqual({ name: 'discover' });
    expect(parsePath('/downloads')).toEqual({ name: 'downloads' });
    expect(parsePath('/reader')).toEqual({ name: 'reader' });
    expect(parsePath('/diagnostics')).toEqual({ name: 'diagnostics' });
  });

  it('falls back to discover for the retired search path', () => {
    expect(parsePath('/search')).toEqual({ name: 'discover' });
  });

  it('opens the lists tab of the library by default', () => {
    expect(parsePath('/library')).toEqual({ name: 'library', library: 'lists' });
    expect(parsePath('/library/status')).toEqual({ name: 'library', library: 'status' });
    expect(parsePath('/library/now-playing')).toEqual({ name: 'library', library: 'now-playing' });
    expect(parsePath('/library/unknown')).toEqual({ name: 'library', library: 'lists' });
  });

  it('decodes a title id, including a kitsu id', () => {
    expect(parsePath('/title/tt0113277')).toEqual({ name: 'title', titleId: 'tt0113277' });
    expect(parsePath('/title/kitsu%3A7442')).toEqual({ name: 'title', titleId: 'kitsu:7442' });
  });

  it('falls back to discover for an empty, a bad, or an unknown path', () => {
    expect(parsePath('/')).toEqual({ name: 'discover' });
    expect(parsePath('/title')).toEqual({ name: 'discover' });
    expect(parsePath('/nothing/here')).toEqual({ name: 'discover' });
    expect(parsePath('/title/%E0%A4%A')).toEqual({ name: 'title', titleId: '%E0%A4%A' });
  });
});

describe('formatPath', () => {
  it('round-trips every route', () => {
    const routes = [
      { name: 'discover' },
      { name: 'library', library: 'status' },
      { name: 'downloads' },
      { name: 'reader' },
      { name: 'diagnostics' },
      { name: 'title', titleId: 'kitsu:7442' },
    ] as const;
    for (const route of routes) {
      expect(parsePath(formatPath(route))).toEqual(route);
    }
  });

  it('never puts a query string or token in a path', () => {
    expect(formatPath({ name: 'title', titleId: 'tt1' })).not.toContain('?');
    expect(formatPath({ name: 'discover' })).toBe('/discover');
  });
});

describe('sameRoute', () => {
  it('compares by path', () => {
    expect(sameRoute({ name: 'library' }, { name: 'library', library: 'lists' })).toBe(true);
    expect(sameRoute({ name: 'library', library: 'status' }, { name: 'library', library: 'lists' })).toBe(false);
  });
});
