export interface CatalogItemSummary {
  id: string;
  name: string;
  media_type: string;
  release_info?: string;
  poster?: string;
  is_anime: boolean;
}

export interface SearchResponse {
  items: CatalogItemSummary[];
  notes: string[];
}

export interface EpisodeSummary {
  id: string;
  stream_id?: string;
  imdb_id?: string;
  title?: string;
  season: number;
  episode: number;
}

export interface EpisodesResponse {
  episodes: EpisodeSummary[];
}

export interface StreamSummary {
  source_id: string;
  name: string;
  file_name?: string;
  file_index?: number;
  quality: string;
  is_recommended: boolean;
  seeders?: number;
  size?: string;
}

export interface StreamsResponse {
  streams: StreamSummary[];
}

export interface PlaySource {
  torrent: string;
  file_index?: number;
  quality: string;
}

export interface EpisodeQueueSeed {
  catalog_item: CatalogItemSummary;
  episodes: EpisodeSummary[];
  current_episode_id: string;
}

export interface MediaRefMovie {
  type: 'movie';
  catalog_id: string;
  title: string;
}

export interface MediaRefEpisode {
  type: 'episode';
  catalog_id: string;
  stream_id: string;
  imdb_id?: string;
  season: number;
  episode: number;
  title?: string;
}

export type MediaRef = MediaRefMovie | MediaRefEpisode;

export interface PlayCommand {
  source_id?: string;
  magnet?: string;
  file_index?: number;
  media_ref?: MediaRef;
  queue_seed?: EpisodeQueueSeed;
  alternatives?: PlaySource[];
}

export interface SafeError {
  code: string;
  message: string;
  retryable: boolean;
}

export type PlaybackSnapshot =
  | { state: 'idle' }
  | { state: 'resolving_source'; media: MediaRef }
  | { state: 'loading_torrent'; media: MediaRef; quality: string }
  | { state: 'buffering'; media: MediaRef; quality: string; file_name: string }
  | { state: 'finding_subtitle'; media: MediaRef; quality: string }
  | { state: 'downloading_subtitle'; media: MediaRef; quality: string; subtitle_name: string }
  | { state: 'launching_player'; media: MediaRef; quality: string; player_name: string }
  | {
      state: 'playing';
      media: MediaRef;
      title: string;
      quality: string;
      file_length: number;
      player_name: string;
      subtitle_ready: boolean;
      has_next: boolean;
    }
  | { state: 'stopping' }
  | { state: 'failed'; error: SafeError };

export type PlaybackState = PlaybackSnapshot;

export interface PlaybackStatusResponse {
  state: PlaybackSnapshot;
}

export interface OperationAccepted {
  operation_id: string;
  status: string;
}

export interface DownloadEntrySummary {
  info_hash: string;
  display_name: string;
  title?: string;
  year?: number;
  file_size?: number;
}

export interface DownloadsResponse {
  entries: DownloadEntrySummary[];
}

export interface PlayerSummary {
  name: string;
  path: string;
  is_available: boolean;
}

export interface SettingsResponse {
  download_dir: string;
  data_dir: string;
  players: PlayerSummary[];
  opensubtitles_configured: boolean;
}

export interface ActivationStatus {
  is_activated: boolean;
  gateway_url?: string;
}

export interface RedeemInviteResponse {
  success: boolean;
  message: string;
}

export interface VlcGuidance {
  platform: string;
  is_installed: boolean;
  command?: string;
  download_url: string;
  instructions: string[];
}

export interface DiagnosticsReport {
  app_version: string;
  vlc_status: string;
  player_path?: string;
  download_dir: string;
  data_dir: string;
  active_session: boolean;
}
