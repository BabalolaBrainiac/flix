export interface CatalogItemSummary {
  id: string;
  name: string;
  media_type: string;
  release_info?: string;
  is_anime: boolean;
}

export interface SearchResponse {
  items: CatalogItemSummary[];
  notes: string[];
}

export interface EpisodeSummary {
  id: string;
  stream_id?: string;
  title?: string;
  season: number;
  episode: number;
}

export interface EpisodesResponse {
  episodes: EpisodeSummary[];
}

export interface StreamSummary {
  info_hash: string;
  name: string;
  file_name?: string;
  file_index?: number;
  quality: string;
  is_recommended: boolean;
  seeders?: number;
  size?: string;
  magnet: string;
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

export interface PlayCommand {
  magnet: string;
  file_index?: number;
  queue_seed?: EpisodeQueueSeed;
  alternatives?: PlaySource[];
}

export type PlaybackState =
  | { status: 'idle' }
  | { status: 'preparing'; title: string; quality: string }
  | {
      status: 'playing';
      title: string;
      quality: string;
      url: string;
      subtitle_url?: string;
      file_length: number;
      player_path: string;
      has_next: boolean;
    }
  | { status: 'stopped' }
  | { status: 'error'; message: string };

export interface PlaybackStatusResponse {
  state: PlaybackState;
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
  vlc_installed: boolean;
  vlc_path?: string;
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
  is_activated: boolean;
  gateway_reachable: boolean;
  vlc_status: string;
  player_path?: string;
  download_dir: string;
  data_dir: string;
  active_session: boolean;
}
