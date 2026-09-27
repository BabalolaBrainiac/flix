import { describe, it, expect } from 'vitest';
import { buildSearchUrl, selectBestCandidate } from '../src/routes/subtitles';

describe('buildSearchUrl', () => {
  it('scopes an episode search by the show, not the episode ID', () => {
    const url = buildSearchUrl('7124066', true, 1, 5);
    expect(url.searchParams.get('parent_imdb_id')).toBe('7124066');
    expect(url.searchParams.get('imdb_id')).toBeNull();
    expect(url.searchParams.get('season_number')).toBe('1');
    expect(url.searchParams.get('episode_number')).toBe('5');
  });

  it('scopes a movie search by its own ID', () => {
    const url = buildSearchUrl('0111161', false, undefined, undefined);
    expect(url.searchParams.get('imdb_id')).toBe('0111161');
    expect(url.searchParams.get('parent_imdb_id')).toBeNull();
    expect(url.searchParams.get('season_number')).toBeNull();
    expect(url.searchParams.get('episode_number')).toBeNull();
  });
});

describe('selectBestCandidate', () => {
  const episodeFive = {
    attributes: {
      feature_details: { season_number: 1, episode_number: 5 },
      files: [{ file_id: 111 }],
      release: 'Modern.Family.S01E05',
    },
  };
  const episodeSix = {
    attributes: {
      feature_details: { season_number: 1, episode_number: 6 },
      files: [{ file_id: 222 }],
      release: 'Modern.Family.S01E06',
    },
  };

  it('skips a result for the wrong episode even when it is ranked first', () => {
    const result = selectBestCandidate([episodeSix, episodeFive], 1, 5);
    expect(result?.file_id).toBe(111);
  });

  it('skips a result for the wrong season', () => {
    const wrongSeason = {
      attributes: {
        feature_details: { season_number: 2, episode_number: 5 },
        files: [{ file_id: 333 }],
      },
    };
    const result = selectBestCandidate([wrongSeason, episodeFive], 1, 5);
    expect(result?.file_id).toBe(111);
  });

  it('returns null when nothing matches the requested episode', () => {
    expect(selectBestCandidate([episodeSix], 1, 5)).toBeNull();
  });

  it('accepts any candidate with a file for a movie lookup', () => {
    const movie = { attributes: { files: [{ file_id: 999 }] } };
    expect(selectBestCandidate([movie], undefined, undefined)?.file_id).toBe(999);
  });

  it('rejects a candidate with no downloadable file', () => {
    const noFile = { attributes: { feature_details: { season_number: 1, episode_number: 5 }, files: [] } };
    expect(selectBestCandidate([noFile], 1, 5)).toBeNull();
  });
});
