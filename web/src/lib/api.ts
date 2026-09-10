import type {
  DownloadsResponse,
  EpisodesResponse,
  OperationAccepted,
  PlayCommand,
  PlaybackSnapshot,
  PlaybackStatusResponse,
  ReaderChaptersResponse,
  ReaderPagesResponse,
  ReaderProgressResponse,
  ReaderSearchResponse,
  RecommendCommand,
  RecommendResponse,
  SearchResponse,
  SettingsResponse,
  StreamsResponse,
} from './types';

let memoryToken: string | null = null;

function initToken(): string | null {
  if (typeof window === 'undefined') return null;
  const params = new URLSearchParams(window.location.search);
  const urlToken = params.get('token');
  if (urlToken) {
    memoryToken = urlToken;
    try {
      sessionStorage.setItem('flix_token', urlToken);
    } catch {
      // Ignore storage errors in restricted contexts
    }
    // Remove token from visible browser URL for security
    const cleanUrl = window.location.pathname + window.location.hash;
    window.history.replaceState({}, document.title, cleanUrl);
    return urlToken;
  }
  if (!memoryToken) {
    try {
      memoryToken = sessionStorage.getItem('flix_token');
    } catch {
      memoryToken = null;
    }
  }
  return memoryToken;
}

export function getToken(): string | null {
  if (!memoryToken) {
    return initToken();
  }
  return memoryToken;
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const token = getToken();
  const headers = new Headers(options.headers || {});
  headers.set('Content-Type', 'application/json');
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
    headers.set('X-Flix-Token', token);
  }

  const response = await fetch(path, {
    ...options,
    headers,
  });

  if (!response.ok) {
    let errorText: string;
    try {
      const errorJson = await response.json();
      errorText = errorJson.error?.message || errorJson.error || JSON.stringify(errorJson);
    } catch {
      errorText = await response.text().catch(() => 'Unknown server error');
    }
    throw new Error(errorText || `Request failed with status ${response.status}`);
  }

  return response.json();
}

export async function getHealth(): Promise<{ status: string; version: string }> {
  return request('/api/health');
}

export async function exportDebugReport(): Promise<unknown> {
  return request('/api/diagnostics/export');
}

export async function getStatus(): Promise<PlaybackStatusResponse> {
  return request('/api/status');
}

export function subscribeEvents(
  onEvent: (snapshot: PlaybackSnapshot) => void,
  onError?: (err: Event) => void
): () => void {
  const token = getToken();
  const url = token ? `/api/events?token=${encodeURIComponent(token)}` : '/api/events';
  const eventSource = new EventSource(url);

  eventSource.onmessage = (event) => {
    try {
      const parsed: PlaybackSnapshot = JSON.parse(event.data);
      onEvent(parsed);
    } catch {
      // Ignore parse error on heartbeat
    }
  };

  if (onError) {
    eventSource.onerror = onError;
  }

  return () => {
    eventSource.close();
  };
}

export async function search(query: string, animeOnly = false): Promise<SearchResponse> {
  return request('/api/search', {
    method: 'POST',
    body: JSON.stringify({ query, anime_only: animeOnly }),
  });
}

export async function getRecommendations(command: RecommendCommand): Promise<RecommendResponse> {
  return request('/api/recommend', {
    method: 'POST',
    body: JSON.stringify(command),
  });
}

export async function searchCatalog(query: string, catalog: 'movie' | 'series' | 'anime', signal: AbortSignal): Promise<SearchResponse> {
  return request('/api/search', {
    method: 'POST',
    signal,
    body: JSON.stringify({ query, catalog }),
  });
}

export async function getEpisodes(itemId: string, isAnime: boolean): Promise<EpisodesResponse> {
  return request('/api/episodes', {
    method: 'POST',
    body: JSON.stringify({ item_id: itemId, is_anime: isAnime }),
  });
}

export async function getStreams(mediaType: string, streamId: string, isAnime = false, signal?: AbortSignal): Promise<StreamsResponse> {
  return request('/api/streams', {
    method: 'POST',
    signal,
    body: JSON.stringify({ media_type: mediaType, stream_id: streamId, is_anime: isAnime }),
  });
}

export async function play(command: PlayCommand): Promise<OperationAccepted> {
  return request('/api/play', {
    method: 'POST',
    body: JSON.stringify(command),
  });
}

export async function nextEpisode(): Promise<PlaybackStatusResponse> {
  return request('/api/next', {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export async function stopPlayback(): Promise<{ success: boolean }> {
  return request('/api/stop', {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export async function getDownloads(): Promise<DownloadsResponse> {
  return request('/api/downloads');
}

export async function addDownload(magnet: string, fileIndex?: number): Promise<DownloadsResponse> {
  return request('/api/downloads', {
    method: 'POST',
    body: JSON.stringify({
      action: 'add',
      magnet,
      file_index: fileIndex,
    }),
  });
}

export async function removeDownload(infoHash: string): Promise<DownloadsResponse> {
  return request('/api/downloads', {
    method: 'POST',
    body: JSON.stringify({
      action: 'remove',
      info_hash: infoHash,
    }),
  });
}

export async function getSettings(): Promise<SettingsResponse> {
  return request('/api/settings');
}

export async function getActivationStatus(): Promise<import('./types').ActivationStatus> {
  return request('/api/activation');
}

export async function redeemInvite(
  inviteCode: string,
  gatewayUrl?: string
): Promise<import('./types').RedeemInviteResponse> {
  return request('/api/activation/redeem', {
    method: 'POST',
    body: JSON.stringify({
      invite_code: inviteCode,
      gateway_url: gatewayUrl,
    }),
  });
}

export async function resetActivation(): Promise<{ success: boolean }> {
  return request('/api/activation/reset', {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export async function getVlcGuidance(): Promise<import('./types').VlcGuidance> {
  return request('/api/vlc-guidance');
}

export async function getDiagnostics(): Promise<import('./types').DiagnosticsReport> {
  return request('/api/diagnostics');
}

export async function quitApp(): Promise<{ success: boolean }> {
  return request('/api/quit', {
    method: 'POST',
    body: JSON.stringify({}),
  });
}

export async function readerSearch(query: string, lang = 'en'): Promise<ReaderSearchResponse> {
  return request('/api/reader/search', {
    method: 'POST',
    body: JSON.stringify({ query, lang }),
  });
}

export async function readerChapters(mangaId: string, lang = 'en'): Promise<ReaderChaptersResponse> {
  return request('/api/reader/chapters', {
    method: 'POST',
    body: JSON.stringify({ manga_id: mangaId, lang }),
  });
}

export async function readerPages(chapterId: string, mangaId: string, lang = 'en'): Promise<ReaderPagesResponse> {
  return request('/api/reader/pages', {
    method: 'POST',
    body: JSON.stringify({ chapter_id: chapterId, manga_id: mangaId, lang }),
  });
}

export async function readerGetProgress(publicationId: string): Promise<ReaderProgressResponse> {
  return request(`/api/reader/progress?publication_id=${encodeURIComponent(publicationId)}`);
}

export async function readerSaveProgress(publicationId: string, chapterId: string, page: number): Promise<void> {
  return request('/api/reader/progress', {
    method: 'POST',
    body: JSON.stringify({ publication_id: publicationId, chapter_id: chapterId, page }),
  });
}

export async function browserEvent(playbackId: string, event: import('./types').BrowserEvent, positionMs = 0): Promise<void> {
  await request('/api/browser/event', {
    method: 'POST',
    body: JSON.stringify({ playback_id: playbackId, event, position_ms: positionMs }),
    signal: AbortSignal.timeout(10_000),
  });
}
