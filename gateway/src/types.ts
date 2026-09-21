export interface Env {
  DB: D1Database;
  OPENSUBTITLES_COORDINATOR: DurableObjectNamespace;
  OPENSUBTITLES_API_KEY?: string;
  OPENSUBTITLES_USER_AGENT?: string;
  OPENSUBTITLES_USERNAME?: string;
  OPENSUBTITLES_PASSWORD?: string;
  // Bearer secret that guards the admin routes. When unset, the admin API is
  // disabled and every admin request is refused.
  ADMIN_SECRET?: string;
}

export interface InviteRecord {
  code_hash: string;
  created_at: number;
  max_devices: number;
  redeemed_count: number;
  is_revoked: number;
  // Short admin-facing tag (e.g. "qa", "users-batch-1"). Never shown to the
  // person redeeming the invite - purely for the admin's own bookkeeping.
  label: string | null;
}

export interface DeviceRecord {
  token_hash: string;
  invite_code_hash: string;
  created_at: number;
  last_used_at: number;
  is_revoked: number;
  request_count: number;
}

export interface ProfileRecord {
  id: string;
  invite_code_hash: string;
  name: string;
  avatar_key: string;
  created_at: number;
  updated_at: number;
}

export interface ListRecordRow {
  id: string;
  profile_id: string;
  name: string;
  created_at: number;
}

export interface ListItemRecord {
  list_id: string;
  catalog_id: string;
  media_type: string;
  added_at: number;
  title: string | null;
  poster: string | null;
  year: number | null;
  canonical_id: string | null;
}

export interface WatchStatusRecord {
  profile_id: string;
  catalog_id: string;
  status: string;
  episode_id: string | null;
  resume_seconds: number | null;
  updated_at: number;
  title: string | null;
  poster: string | null;
  media_type: string | null;
  year: number | null;
  canonical_id: string | null;
}

export interface DeletionRecord {
  kind: string;
  key: string;
  deleted_at: number;
}

export interface SubtitleResolveRequest {
  imdb_id: string;
  season?: number;
  episode?: number;
}

export interface CatalogSearchItem {
  id: string;
  name: string;
  media_type: string;
  release_info?: string;
  is_anime: boolean;
}

export type SubtitleResolveResponse =
  | {
      status: 'matched';
      file_id: number;
      file_name?: string;
      content: string;
      format: 'srt';
    }
  | {
      status: 'no_match';
      reason: string;
    };
