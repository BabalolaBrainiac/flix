import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchMetadata } from '../src/upstream';

afterEach(() => vi.unstubAllGlobals());

describe('Provider response limits', () => {
  it('rejects a large response without a declared length', async () => {
    const cancel = vi.fn();
    const body = new ReadableStream({
      start(controller) {
        controller.enqueue(new Uint8Array(4 * 1024 * 1024 + 1));
      },
      cancel,
    });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body)));
    await expect(fetchMetadata('https://example.org')).rejects.toThrow('size limit');
    expect(cancel).toHaveBeenCalledOnce();
  });

  it('keeps a small response and applies a deadline', async () => {
    const fetch = vi.fn().mockResolvedValue(Response.json({ items: [] }));
    vi.stubGlobal('fetch', fetch);
    expect(await (await fetchMetadata('https://example.org')).json()).toEqual({ items: [] });
    expect(fetch.mock.calls[0][1].signal).toBeInstanceOf(AbortSignal);
    expect(fetch.mock.calls[0][1].redirect).toBe('error');
  });
});
