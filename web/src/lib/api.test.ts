import { describe, it, expect, beforeEach, vi } from 'vitest';
import { getHealth, search, play, getDiagnostics } from './api';

describe('API Client', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('injects authorization header when token exists', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ status: 'ok', version: '0.1.0' }),
    });
    globalThis.fetch = mockFetch;

    const res = await getHealth();
    expect(res.status).toBe('ok');
    expect(mockFetch).toHaveBeenCalledTimes(1);

    const callArgs = mockFetch.mock.calls[0];
    expect(callArgs[0]).toBe('/api/health');
  });

  it('formats search payload with anime_only flag', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [], notes: [] }),
    });
    globalThis.fetch = mockFetch;

    await search('Naruto', true);
    expect(mockFetch).toHaveBeenCalledTimes(1);

    const callArgs = mockFetch.mock.calls[0];
    expect(callArgs[0]).toBe('/api/search');
    expect(callArgs[1].method).toBe('POST');
    expect(JSON.parse(callArgs[1].body)).toEqual({
      query: 'Naruto',
      anime_only: true,
    });
  });

  it('formats play payload with magnet and alternatives', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ state: { status: 'playing' } }),
    });
    globalThis.fetch = mockFetch;

    await play({
      magnet: 'magnet:?xt=urn:btih:1234567890abcdef1234567890abcdef12345678',
      file_index: 0,
      alternatives: [
        {
          torrent: 'magnet:?xt=urn:btih:abcdef1234567890abcdef1234567890abcdef12',
          file_index: 0,
          quality: '1080p',
        },
      ],
    });

    expect(mockFetch).toHaveBeenCalledTimes(1);
    const callArgs = mockFetch.mock.calls[0];
    expect(callArgs[0]).toBe('/api/play');
    expect(callArgs[1].method).toBe('POST');
    const body = JSON.parse(callArgs[1].body);
    expect(body.magnet).toContain('1234567890');
    expect(body.alternatives.length).toBe(1);
    expect(body.alternatives[0].quality).toBe('1080p');
  });

  it('fetches system diagnostics report', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        app_version: '0.1.0',
        vlc_status: 'Installed',
      }),
    });
    globalThis.fetch = mockFetch;

    const diag = await getDiagnostics();
    expect(diag.app_version).toBe('0.1.0');
    expect(diag.vlc_status).toBe('Installed');
  });

  it('throws error when server responds with non-ok status', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => 'Unauthorized token',
    });
    globalThis.fetch = mockFetch;

    await expect(getHealth()).rejects.toThrow('Unauthorized token');
  });

  it('calls profile endpoints with proper routes and bodies', async () => {
    const { getProfiles, createProfile, setActiveProfile } = await import('./api');
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ profiles: [], active_profile_id: 'default' }),
    });
    globalThis.fetch = mockFetch;

    await getProfiles();
    expect(mockFetch).toHaveBeenLastCalledWith('/api/profiles', expect.anything());

    await createProfile('Family', 'family_group');
    expect(mockFetch).toHaveBeenLastCalledWith(
      '/api/profiles',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'Family', avatar_key: 'family_group' }),
      }),
    );

    await setActiveProfile('p-123');
    expect(mockFetch).toHaveBeenLastCalledWith(
      '/api/profiles/active',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ profile_id: 'p-123' }),
      }),
    );
  });

  it('calls list and watch status endpoints with profile query param', async () => {
    const { getLists, getWatchStatus, setWatchStatus } = await import('./api');
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ lists: [], statuses: [] }),
    });
    globalThis.fetch = mockFetch;

    await getLists('prof-1');
    expect(mockFetch).toHaveBeenLastCalledWith('/api/profiles/lists?profile_id=prof-1', expect.anything());

    await getWatchStatus('prof-1');
    expect(mockFetch).toHaveBeenLastCalledWith('/api/profiles/status?profile_id=prof-1', expect.anything());

    await setWatchStatus('prof-1', { catalogId: 'm-1', mediaType: 'movie', title: 'Heat', year: 1995 }, 'completed', undefined, 8800);
    expect(mockFetch).toHaveBeenLastCalledWith(
      '/api/profiles/status',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          profile_id: 'prof-1',
          catalog_id: 'm-1',
          status: 'completed',
          resume_seconds: 8800,
          title: 'Heat',
          media_type: 'movie',
          year: 1995,
        }),
      }),
    );
  });

  it('sends the explicit delete route to remove a watch status', async () => {
    const { removeWatchStatus } = await import('./api');
    const mockFetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ success: true }) });
    globalThis.fetch = mockFetch;

    await removeWatchStatus('prof-1', 'm-1');
    expect(mockFetch).toHaveBeenLastCalledWith(
      '/api/profiles/status/delete',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ profile_id: 'prof-1', catalog_id: 'm-1' }),
      }),
    );
  });

  it('calls check for update endpoint via GET', async () => {
    const { checkForUpdate } = await import('./api');
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        current_version: '0.4.0',
        latest_version: '0.4.1',
        available: true,
        release_url: 'https://github.com/BabalolaBrainiac/flix/releases/tag/v0.4.1',
      }),
    });
    globalThis.fetch = mockFetch;

    const res = await checkForUpdate();
    expect(res.available).toBe(true);
    expect(res.latest_version).toBe('0.4.1');
    expect(mockFetch).toHaveBeenLastCalledWith('/api/update', expect.anything());
  });

  it('calls install update endpoint via POST with empty body', async () => {
    const { installUpdate } = await import('./api');
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        version: '0.4.1',
        package_path: '/path/to/update',
        requires_manual_finish: false,
      }),
    });
    globalThis.fetch = mockFetch;

    const res = await installUpdate();
    expect(res.version).toBe('0.4.1');
    expect(res.requires_manual_finish).toBe(false);
    expect(mockFetch).toHaveBeenLastCalledWith(
      '/api/update',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({}),
      }),
    );
  });
});
