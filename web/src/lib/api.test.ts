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
});
