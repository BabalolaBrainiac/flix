import type {
  DownloadsResponse,
  EpisodesResponse,
  PlayCommand,
  PlaybackStatusResponse,
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
    const errorText = await response.text().catch(() => 'Unknown server error');
    throw new Error(errorText || `Request failed with status ${response.status}`);
  }

  return response.json();
}

export async function getHealth(): Promise<{ status: string; version: string }> {
  return request('/api/health');
}

export async function getStatus(): Promise<PlaybackStatusResponse> {
  return request('/api/status');
}

export async function search(query: string, animeOnly = false): Promise<SearchResponse> {
  return request('/api/search', {
    method: 'POST',
    body: JSON.stringify({ query, anime_only: animeOnly }),
  });
}

export async function getEpisodes(itemId: string, isAnime: boolean): Promise<EpisodesResponse> {
  return request('/api/episodes', {
    method: 'POST',
    body: JSON.stringify({ item_id: itemId, is_anime: isAnime }),
  });
}

export async function getStreams(mediaType: string, streamId: string): Promise<StreamsResponse> {
  return request('/api/streams', {
    method: 'POST',
    body: JSON.stringify({ media_type: mediaType, stream_id: streamId }),
  });
}

export async function play(command: PlayCommand): Promise<PlaybackStatusResponse> {
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

