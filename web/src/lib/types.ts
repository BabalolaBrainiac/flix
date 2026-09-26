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

export interface RecommendCommand {
  keywords: string;
  show?: boolean;
  movie?: boolean;
  anime?: boolean;
  limit?: number;
  min_rating?: number;
  since_year?: number;
  offset?: number;
}

export interface RecommendItem {
  id: string;
  name: string;
  media_type: string;
  release_info: string | null;
  poster: string | null;
  genres: string[] | null;
  imdb_rating: string | null;
  description: string | null;
  is_anime: boolean;
  score: number;
}

export interface RecommendResponse {
  items: RecommendItem[];
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
  is_anime?: boolean;
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
  is_anime?: boolean;
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
  target?: 'external' | 'browser';
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

export type BrowserEvent = 'playing' | 'paused' | 'buffering' | 'heartbeat' | 'ended' | 'failed' | 'stop';
export interface BrowserPlayback {
  state: 'browser';
  media: MediaRef;
  title: string;
  quality: string;
  playback_id: string;
  stream_url: string;
  mime_type: string;
  subtitle_url: string | null;
  phase: 'ready' | 'playing' | 'paused' | 'buffering';
  position_ms: number;
  has_next: boolean;
}

export type PlaybackSnapshot =
  | BrowserPlayback
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

export interface UpdateStatus {
  current_version: string;
  latest_version: string;
  available: boolean;
  release_url: string;
}

export interface UpdateInstallResult {
  version: string;
  package_path: string;
  requires_manual_finish: boolean;
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

export interface ReaderPublication {
  id: string;
  title: string;
  description: string | null;
  year: number | null;
  status: string | null;
  tags: string[];
  cover_url: string | null;
  source: string;
}

export interface ReaderChapter {
  id: string;
  chapter: string | null;
  volume: string | null;
  title: string | null;
  language: string;
  pages: number;
  external_url: string | null;
}

export interface ReaderSearchResponse {
  publications: ReaderPublication[];
}

export interface ReaderChaptersResponse {
  chapters: ReaderChapter[];
  available_languages: string[];
}

export interface ReaderPagesResponse {
  chapter_id: string;
  page_count: number;
  pages: string[];
}

export interface ReaderProgress {
  publication_id: string;
  chapter_id: string;
  page: number;
  updated_at: string;
}

export interface ReaderProgressResponse {
  position: ReaderProgress | null;
}

export interface ProfileSummary {
  id: string;
  name: string;
  avatar_key: string;
  created_at: number;
  updated_at: number;
}

export interface ProfilesResponse {
  profiles: ProfileSummary[];
  active_profile_id: string | null;
}

export interface ListItemSummary {
  catalog_id: string;
  media_type: string;
  added_at: number;
  title?: string | null;
  poster?: string | null;
  year?: number | null;
  canonical_id?: string | null;
}

export interface ListSummary {
  id: string;
  profile_id: string;
  name: string;
  created_at: number;
  items: ListItemSummary[];
}

export interface ListsResponse {
  lists: ListSummary[];
}

export interface WatchStatusSummary {
  catalog_id: string;
  status: 'to_watch' | 'in_progress' | 'watched' | string;
  episode_id: string | null;
  resume_seconds: number | null;
  updated_at: number;
  title?: string | null;
  poster?: string | null;
  media_type?: string | null;
  year?: number | null;
  canonical_id?: string | null;
}

export interface WatchStatusResponse {
  watch_status: WatchStatusSummary[];
}

/** What the app needs to know about a title to store it in a list or status. */
export interface TitleRef {
  catalogId: string;
  mediaType: string;
  title?: string | null;
  poster?: string | null;
  year?: number | null;
  /** The IMDb id of the same title, when known. It joins a `kitsu:` id to a `tt` id. */
  canonicalId?: string | null;
}
