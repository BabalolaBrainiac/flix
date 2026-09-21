//! Syncs profiles, lists, and watch status with the gateway, when one is
//! configured.
//!
//! Flix works fully offline: this module only ever runs as a best-effort
//! background task, never on the catalog search or playback path. A failure
//! here never surfaces as a blocking error. The local `ProfileStore` stays
//! the source of truth for the device until the next successful sync.
//!
//! **Merge rule.** For each id, the copy with the newer timestamp wins. A
//! deletion is a record with a timestamp (a tombstone). A tombstone that is
//! newer than the other side's copy deletes that copy. A copy that is newer
//! than the tombstone survives. Without tombstones, a row that one device
//! removed would come back from the other device on the next sync.

use crate::desktop::{credentials, gateway};
use crate::profiles::{
    list_item_key, watch_status_key, Deletion, ListItem, ListRecord, Profile, ProfileStore,
    TitleMeta, WatchState, WatchStatus, KIND_LIST, KIND_LIST_ITEM, KIND_PROFILE, KIND_WATCH_STATUS,
};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteProfile {
    pub id: String,
    pub name: String,
    pub avatar_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteListItem {
    pub catalog_id: String,
    pub media_type: String,
    pub added_at: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub canonical_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteList {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    #[serde(default)]
    pub items: Vec<RemoteListItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteWatchStatus {
    pub catalog_id: String,
    pub status: String,
    pub episode_id: Option<String>,
    pub resume_seconds: Option<i64>,
    pub updated_at: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub canonical_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteDeletion {
    pub kind: String,
    pub key: String,
    pub deleted_at: i64,
}

/// What to do about one id after both sides are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    Push,
    Pull,
    DeleteLocal,
    DeleteRemote,
    NoOp,
}

/// The core sync decision. It has no I/O so it stays easy to test.
///
/// `local` and `remote` are the `updated_at` of each side's live copy, or
/// `None` when that side has no copy. `local_deleted` and `remote_deleted`
/// are the tombstone times, or `None` when the side has no tombstone.
/// Equal copies need no write. A tombstone with the same time as a copy
/// wins, because a delete that follows an add in one millisecond is real.
pub fn resolve(
    local: Option<i64>,
    remote: Option<i64>,
    local_deleted: Option<i64>,
    remote_deleted: Option<i64>,
) -> SyncAction {
    match (local, remote) {
        (Some(l), Some(r)) => match l.cmp(&r) {
            std::cmp::Ordering::Greater => SyncAction::Push,
            std::cmp::Ordering::Less => SyncAction::Pull,
            std::cmp::Ordering::Equal => SyncAction::NoOp,
        },
        (Some(l), None) if remote_deleted.is_some_and(|d| d >= l) => SyncAction::DeleteLocal,
        (Some(_), None) => SyncAction::Push,
        (None, Some(r)) if local_deleted.is_some_and(|d| d >= r) => SyncAction::DeleteRemote,
        (None, Some(_)) => SyncAction::Pull,
        (None, None) => SyncAction::NoOp,
    }
}

/// The deletion records of both sides, loaded once per sync pass.
#[derive(Debug, Default)]
pub struct Tombstones {
    local: HashMap<(String, String), i64>,
    remote: HashMap<(String, String), i64>,
}

impl Tombstones {
    pub fn new(local: Vec<Deletion>, remote: Vec<RemoteDeletion>) -> Self {
        Self {
            local: local
                .into_iter()
                .map(|d| ((d.kind, d.key), d.deleted_at))
                .collect(),
            remote: remote
                .into_iter()
                .map(|d| ((d.kind, d.key), d.deleted_at))
                .collect(),
        }
    }

    fn local_at(&self, kind: &str, key: &str) -> Option<i64> {
        self.local
            .get(&(kind.to_string(), key.to_string()))
            .copied()
    }

    fn remote_at(&self, kind: &str, key: &str) -> Option<i64> {
        self.remote
            .get(&(kind.to_string(), key.to_string()))
            .copied()
    }
}

/// Compares two id-to-timestamp maps and returns the ids that need work.
/// `key_prefix` turns an id into its tombstone key (for example
/// `"{list_id}:"` for list items).
pub fn plan(
    kind: &str,
    key_prefix: &str,
    local: &HashMap<String, i64>,
    remote: &HashMap<String, i64>,
    tombs: &Tombstones,
) -> Vec<(String, SyncAction)> {
    let mut ids: Vec<&String> = local.keys().chain(remote.keys()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            let key = format!("{key_prefix}{id}");
            let action = resolve(
                local.get(id).copied(),
                remote.get(id).copied(),
                tombs.local_at(kind, &key),
                tombs.remote_at(kind, &key),
            );
            (action != SyncAction::NoOp).then(|| (id.clone(), action))
        })
        .collect()
}

/// The gateway calls that sync needs. A trait so tests can run two devices
/// against one in-memory gateway.
#[allow(async_fn_in_trait)]
pub trait SyncRemote {
    async fn deletions(&self) -> Result<Vec<RemoteDeletion>>;
    async fn list_profiles(&self) -> Result<Vec<RemoteProfile>>;
    async fn push_profile(&self, profile: &Profile) -> Result<()>;
    async fn delete_profile(&self, profile_id: &str) -> Result<()>;
    async fn list_lists(&self, profile_id: &str) -> Result<Vec<RemoteList>>;
    async fn create_list(&self, list: &ListRecord) -> Result<()>;
    async fn delete_list(&self, list_id: &str) -> Result<()>;
    async fn add_list_item(&self, item: &ListItem) -> Result<()>;
    async fn remove_list_item(&self, list_id: &str, catalog_id: &str) -> Result<()>;
    async fn list_watch_status(&self, profile_id: &str) -> Result<Vec<RemoteWatchStatus>>;
    async fn set_watch_status(&self, status: &WatchStatus) -> Result<()>;
    async fn delete_watch_status(&self, profile_id: &str, catalog_id: &str) -> Result<()>;
}

pub struct GatewaySyncClient {
    client: reqwest::Client,
    base_url: String,
    device_token: String,
}

impl GatewaySyncClient {
    /// Builds a client from the saved activation, or `None` when the device
    /// has never been activated. Sync does not run in that case.
    pub fn from_saved_activation() -> Result<Option<Self>> {
        let Some(device_token) = credentials::get_device_token()? else {
            return Ok(None);
        };
        let base_url =
            gateway::saved_base_url().unwrap_or_else(|| gateway::PACKAGED_GATEWAY_URL.to_string());
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to create the sync gateway client")?;
        Ok(Some(Self {
            client,
            base_url,
            device_token,
        }))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<T> {
        let response = builder
            .header("Authorization", format!("Bearer {}", self.device_token))
            .send()
            .await
            .context("Gateway sync request failed")?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Gateway sync request returned {}",
                response.status()
            ));
        }
        response
            .json::<T>()
            .await
            .context("Gateway sync returned an invalid response")
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<()> {
        let _: serde_json::Value = self
            .send(self.client.post(self.url(path)).json(&body))
            .await?;
        Ok(())
    }
}

impl SyncRemote for GatewaySyncClient {
    async fn deletions(&self) -> Result<Vec<RemoteDeletion>> {
        #[derive(Deserialize)]
        struct Resp {
            deletions: Vec<RemoteDeletion>,
        }
        let resp: Resp = self
            .send(self.client.get(self.url("/v1/profiles/deletions")))
            .await?;
        Ok(resp.deletions)
    }

    async fn list_profiles(&self) -> Result<Vec<RemoteProfile>> {
        #[derive(Deserialize)]
        struct Resp {
            profiles: Vec<RemoteProfile>,
        }
        let resp: Resp = self.send(self.client.get(self.url("/v1/profiles"))).await?;
        Ok(resp.profiles)
    }

    async fn push_profile(&self, profile: &Profile) -> Result<()> {
        self.post(
            "/v1/profiles",
            serde_json::json!({
                "id": profile.id,
                "name": profile.name,
                "avatar_key": profile.avatar_key,
                "created_at": profile.created_at,
                "updated_at": profile.updated_at,
            }),
        )
        .await
    }

    async fn delete_profile(&self, profile_id: &str) -> Result<()> {
        self.post(
            "/v1/profiles/delete",
            serde_json::json!({ "profile_id": profile_id }),
        )
        .await
    }

    async fn list_lists(&self, profile_id: &str) -> Result<Vec<RemoteList>> {
        #[derive(Deserialize)]
        struct Resp {
            lists: Vec<RemoteList>,
        }
        let resp: Resp = self
            .send(
                self.client
                    .get(self.url("/v1/profiles/lists"))
                    .query(&[("profile_id", profile_id)]),
            )
            .await?;
        Ok(resp.lists)
    }

    async fn create_list(&self, list: &ListRecord) -> Result<()> {
        self.post(
            "/v1/profiles/lists",
            serde_json::json!({
                "id": list.id,
                "profile_id": list.profile_id,
                "name": list.name,
                "created_at": list.created_at,
            }),
        )
        .await
    }

    async fn delete_list(&self, list_id: &str) -> Result<()> {
        self.post(
            "/v1/profiles/lists/delete",
            serde_json::json!({ "list_id": list_id }),
        )
        .await
    }

    async fn add_list_item(&self, item: &ListItem) -> Result<()> {
        self.post(
            "/v1/profiles/lists/items",
            serde_json::json!({
                "list_id": item.list_id,
                "catalog_id": item.catalog_id,
                "media_type": item.media_type,
                "added_at": item.added_at,
                "title": item.title,
                "poster": item.poster,
                "year": item.year,
                "canonical_id": item.canonical_id,
            }),
        )
        .await
    }

    async fn remove_list_item(&self, list_id: &str, catalog_id: &str) -> Result<()> {
        self.post(
            "/v1/profiles/lists/items/delete",
            serde_json::json!({ "list_id": list_id, "catalog_id": catalog_id }),
        )
        .await
    }

    async fn list_watch_status(&self, profile_id: &str) -> Result<Vec<RemoteWatchStatus>> {
        #[derive(Deserialize)]
        struct Resp {
            watch_status: Vec<RemoteWatchStatus>,
        }
        let resp: Resp = self
            .send(
                self.client
                    .get(self.url("/v1/profiles/status"))
                    .query(&[("profile_id", profile_id)]),
            )
            .await?;
        Ok(resp.watch_status)
    }

    async fn set_watch_status(&self, status: &WatchStatus) -> Result<()> {
        self.post(
            "/v1/profiles/status",
            serde_json::json!({
                "profile_id": status.profile_id,
                "catalog_id": status.catalog_id,
                "status": status.status.as_str(),
                "episode_id": status.episode_id,
                "resume_seconds": status.resume_seconds,
                "updated_at": status.updated_at,
                "title": status.title,
                "poster": status.poster,
                "media_type": status.media_type,
                "year": status.year,
                "canonical_id": status.canonical_id,
            }),
        )
        .await
    }

    async fn delete_watch_status(&self, profile_id: &str, catalog_id: &str) -> Result<()> {
        self.post(
            "/v1/profiles/status/delete",
            serde_json::json!({ "profile_id": profile_id, "catalog_id": catalog_id }),
        )
        .await
    }
}

/// Lets one sync run at a time, and at most one start in each `min_gap`. A
/// screen that reads its lists on every open then cannot start a sync storm.
pub struct SyncGate {
    state: std::sync::Mutex<GateState>,
    min_gap: Duration,
}

struct GateState {
    running: bool,
    last_start: Option<std::time::Instant>,
}

/// Held for the length of one sync. Dropping it lets the next sync start.
pub struct SyncPermit(Arc<SyncGate>);

impl SyncGate {
    pub fn new(min_gap: Duration) -> Arc<Self> {
        Arc::new(Self {
            state: std::sync::Mutex::new(GateState {
                running: false,
                last_start: None,
            }),
            min_gap,
        })
    }

    /// Returns a permit when a sync may start now, or `None` when one runs
    /// or one started less than `min_gap` ago.
    pub fn try_start(self: &Arc<Self>) -> Option<SyncPermit> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let too_soon = state
            .last_start
            .is_some_and(|started| started.elapsed() < self.min_gap);
        if state.running || too_soon {
            return None;
        }
        state.running = true;
        state.last_start = Some(std::time::Instant::now());
        Some(SyncPermit(Arc::clone(self)))
    }
}

impl Drop for SyncPermit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        state.running = false;
    }
}

/// Runs a local store call on the blocking pool, so a disk write never
/// stalls the async runtime that playback also uses.
async fn blocking<T, F>(local: &Arc<ProfileStore>, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&ProfileStore) -> Result<T> + Send + 'static,
{
    let local = Arc::clone(local);
    tokio::task::spawn_blocking(move || work(&local))
        .await
        .context("Local store task panicked")?
}

fn remote_meta(item: &RemoteListItem) -> TitleMeta<'_> {
    TitleMeta {
        media_type: Some(item.media_type.as_str()),
        title: item.title.as_deref(),
        poster: item.poster.as_deref(),
        year: item.year,
        canonical_id: item.canonical_id.as_deref(),
    }
}

/// Runs one sync pass for every local profile. It never panics and never
/// returns a hard failure. The caller spawns it as a detached background
/// task and must not wait for it on a request path.
pub async fn sync_all<R: SyncRemote>(local: Arc<ProfileStore>, remote: &R) {
    if let Err(error) = run_sync(&local, remote).await {
        tracing::warn!("Sync failed, local data is unaffected: {error:#}");
    }
}

async fn run_sync<R: SyncRemote>(local: &Arc<ProfileStore>, remote: &R) -> Result<()> {
    let local_deletions = blocking(local, |store| store.deletions()).await?;
    let tombs = Tombstones::new(local_deletions, remote.deletions().await?);

    sync_profiles(local, remote, &tombs).await?;
    let profiles = blocking(local, |store| store.list_profiles()).await?;
    for profile in profiles {
        if let Err(error) = sync_lists(local, remote, &profile.id, &tombs).await {
            tracing::warn!("Sync: list sync failed for one profile: {error:#}");
        }
        if let Err(error) = sync_watch_status(local, remote, &profile.id, &tombs).await {
            tracing::warn!("Sync: watch status sync failed for one profile: {error:#}");
        }
    }
    Ok(())
}

async fn apply_local_deletion(
    local: &Arc<ProfileStore>,
    kind: &'static str,
    key: String,
    tombs: &Tombstones,
) -> Result<()> {
    let deleted_at = tombs.remote_at(kind, &key).unwrap_or_default();
    blocking(local, move |store| {
        store.apply_deletion(kind, &key, deleted_at)
    })
    .await
}

async fn sync_profiles<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    tombs: &Tombstones,
) -> Result<()> {
    let local_profiles = blocking(local, |store| store.list_profiles()).await?;
    let remote_profiles = remote.list_profiles().await?;
    let local_times: HashMap<String, i64> = local_profiles
        .iter()
        .map(|p| (p.id.clone(), p.updated_at))
        .collect();
    let remote_times: HashMap<String, i64> = remote_profiles
        .iter()
        .map(|p| (p.id.clone(), p.updated_at))
        .collect();
    let local_by_id: HashMap<&str, &Profile> =
        local_profiles.iter().map(|p| (p.id.as_str(), p)).collect();
    let remote_by_id: HashMap<&str, &RemoteProfile> =
        remote_profiles.iter().map(|p| (p.id.as_str(), p)).collect();

    for (id, action) in plan(KIND_PROFILE, "", &local_times, &remote_times, tombs) {
        match action {
            SyncAction::Push => remote.push_profile(local_by_id[id.as_str()]).await?,
            SyncAction::Pull => {
                let p = remote_by_id[id.as_str()].clone();
                blocking(local, move |store| {
                    store.upsert_profile(&p.id, &p.name, &p.avatar_key, p.created_at, p.updated_at)
                })
                .await?;
            }
            SyncAction::DeleteLocal => apply_local_deletion(local, KIND_PROFILE, id, tombs).await?,
            SyncAction::DeleteRemote => remote.delete_profile(&id).await?,
            SyncAction::NoOp => {}
        }
    }
    Ok(())
}

async fn sync_watch_status<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    profile_id: &str,
    tombs: &Tombstones,
) -> Result<()> {
    let remote_status = remote.list_watch_status(profile_id).await?;
    let local_status = {
        let profile_id = profile_id.to_string();
        blocking(local, move |store| store.all_watch_status(&profile_id)).await?
    };
    let local_times: HashMap<String, i64> = local_status
        .iter()
        .map(|s| (s.catalog_id.clone(), s.updated_at))
        .collect();
    let remote_times: HashMap<String, i64> = remote_status
        .iter()
        .map(|s| (s.catalog_id.clone(), s.updated_at))
        .collect();
    let local_by_id: HashMap<&str, &WatchStatus> = local_status
        .iter()
        .map(|s| (s.catalog_id.as_str(), s))
        .collect();
    let remote_by_id: HashMap<&str, &RemoteWatchStatus> = remote_status
        .iter()
        .map(|s| (s.catalog_id.as_str(), s))
        .collect();
    let prefix = format!("{profile_id}:");

    for (catalog_id, action) in plan(
        KIND_WATCH_STATUS,
        &prefix,
        &local_times,
        &remote_times,
        tombs,
    ) {
        match action {
            SyncAction::Push => {
                remote
                    .set_watch_status(local_by_id[catalog_id.as_str()])
                    .await?
            }
            SyncAction::Pull => {
                pull_watch_status(local, profile_id, remote_by_id[catalog_id.as_str()]).await?
            }
            SyncAction::DeleteLocal => {
                let key = watch_status_key(profile_id, &catalog_id);
                apply_local_deletion(local, KIND_WATCH_STATUS, key, tombs).await?
            }
            SyncAction::DeleteRemote => remote.delete_watch_status(profile_id, &catalog_id).await?,
            SyncAction::NoOp => {}
        }
    }
    Ok(())
}

async fn pull_watch_status(
    local: &Arc<ProfileStore>,
    profile_id: &str,
    item: &RemoteWatchStatus,
) -> Result<()> {
    let state = WatchState::parse(&item.status)?;
    let profile_id = profile_id.to_string();
    let item = item.clone();
    blocking(local, move |store| {
        let meta = TitleMeta {
            media_type: item.media_type.as_deref(),
            title: item.title.as_deref(),
            poster: item.poster.as_deref(),
            year: item.year,
            canonical_id: item.canonical_id.as_deref(),
        };
        store.upsert_watch_status(
            &profile_id,
            &item.catalog_id,
            state,
            item.episode_id.as_deref(),
            item.resume_seconds,
            item.updated_at,
            &meta,
        )
    })
    .await
}

async fn read_lists<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    profile_id: &str,
) -> Result<(Vec<ListRecord>, Vec<RemoteList>)> {
    let remote_lists = remote.list_lists(profile_id).await?;
    let profile_id = profile_id.to_string();
    let local_lists = blocking(local, move |store| store.list_lists(&profile_id)).await?;
    Ok((local_lists, remote_lists))
}

async fn sync_lists<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    profile_id: &str,
    tombs: &Tombstones,
) -> Result<()> {
    let (mut local_lists, mut remote_lists) = read_lists(local, remote, profile_id).await?;
    let collisions = find_name_collisions(&local_lists, &remote_lists, tombs);
    if !collisions.is_empty() {
        for (local_list, remote_list) in &collisions {
            merge_name_collision(local, remote, local_list, remote_list).await?;
        }
        (local_lists, remote_lists) = read_lists(local, remote, profile_id).await?;
    }

    let removed = apply_list_plan(
        local,
        remote,
        profile_id,
        &local_lists,
        &remote_lists,
        tombs,
    )
    .await?;
    let mut list_ids: Vec<&str> = local_lists
        .iter()
        .map(|l| l.id.as_str())
        .chain(remote_lists.iter().map(|l| l.id.as_str()))
        .filter(|id| !removed.contains(*id))
        .collect();
    list_ids.sort_unstable();
    list_ids.dedup();

    let profile = profile_id.to_string();
    let mut local_items: HashMap<String, Vec<ListItem>> = HashMap::new();
    for item in blocking(local, move |store| store.items_for_profile(&profile)).await? {
        local_items
            .entry(item.list_id.clone())
            .or_default()
            .push(item);
    }
    let remote_items: HashMap<&str, &[RemoteListItem]> = remote_lists
        .iter()
        .map(|l| (l.id.as_str(), l.items.as_slice()))
        .collect();

    for list_id in list_ids {
        let local_side = local_items.get(list_id).map(Vec::as_slice).unwrap_or(&[]);
        let remote_side = remote_items.get(list_id).copied().unwrap_or(&[]);
        sync_list_items(local, remote, list_id, local_side, remote_side, tombs).await?;
    }
    Ok(())
}

/// Two lists with one name and different ids: one was made on each side
/// while offline. The list with the older `created_at` stays.
fn find_name_collisions(
    local_lists: &[ListRecord],
    remote_lists: &[RemoteList],
    tombs: &Tombstones,
) -> Vec<(ListRecord, RemoteList)> {
    let local_ids: HashSet<&str> = local_lists.iter().map(|l| l.id.as_str()).collect();
    let remote_ids: HashSet<&str> = remote_lists.iter().map(|l| l.id.as_str()).collect();
    let deleted_remotely = |l: &ListRecord| {
        tombs
            .remote_at(KIND_LIST, &l.id)
            .is_some_and(|d| d >= l.created_at)
    };
    let deleted_locally = |r: &RemoteList| {
        tombs
            .local_at(KIND_LIST, &r.id)
            .is_some_and(|d| d >= r.created_at)
    };

    local_lists
        .iter()
        .filter(|l| !remote_ids.contains(l.id.as_str()) && !deleted_remotely(l))
        .filter_map(|l| {
            remote_lists
                .iter()
                .find(|r| {
                    !local_ids.contains(r.id.as_str())
                        && !deleted_locally(r)
                        && r.name.eq_ignore_ascii_case(&l.name)
                })
                .map(|r| (l.clone(), r.clone()))
        })
        .collect()
}

async fn merge_name_collision<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    local_list: &ListRecord,
    remote_list: &RemoteList,
) -> Result<()> {
    if local_list.created_at <= remote_list.created_at {
        keep_local_list(local, remote, local_list, remote_list).await
    } else {
        keep_remote_list(local, local_list, remote_list).await
    }
}

/// The local list is older. Its copy of the items absorbs the remote list,
/// then the remote list is deleted. The normal pass pushes the merged list.
async fn keep_local_list<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    local_list: &ListRecord,
    remote_list: &RemoteList,
) -> Result<()> {
    let list_id = local_list.id.clone();
    let items = remote_list.items.clone();
    blocking(local, move |store| {
        for item in &items {
            store.upsert_list_item(
                &list_id,
                &item.catalog_id,
                &item.media_type,
                item.added_at,
                &remote_meta(item),
            )?;
        }
        Ok(())
    })
    .await?;
    remote.delete_list(&remote_list.id).await
}

/// The remote list is older. The local list is replaced by it, and the local
/// items move to it. The normal pass pushes those items.
async fn keep_remote_list(
    local: &Arc<ProfileStore>,
    local_list: &ListRecord,
    remote_list: &RemoteList,
) -> Result<()> {
    let old_id = local_list.id.clone();
    let profile_id = local_list.profile_id.clone();
    let remote_list = remote_list.clone();
    blocking(local, move |store| {
        store.replace_list(
            &old_id,
            &remote_list.id,
            &profile_id,
            &remote_list.name,
            remote_list.created_at,
        )?;
        for item in &remote_list.items {
            store.upsert_list_item(
                &remote_list.id,
                &item.catalog_id,
                &item.media_type,
                item.added_at,
                &remote_meta(item),
            )?;
        }
        Ok(())
    })
    .await
}

/// Applies the list-level actions. Returns the ids of lists that were
/// deleted on either side, so their items are not synced.
async fn apply_list_plan<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    profile_id: &str,
    local_lists: &[ListRecord],
    remote_lists: &[RemoteList],
    tombs: &Tombstones,
) -> Result<HashSet<String>> {
    let local_times: HashMap<String, i64> = local_lists
        .iter()
        .map(|l| (l.id.clone(), l.created_at))
        .collect();
    let remote_times: HashMap<String, i64> = remote_lists
        .iter()
        .map(|l| (l.id.clone(), l.created_at))
        .collect();
    let local_by_id: HashMap<&str, &ListRecord> =
        local_lists.iter().map(|l| (l.id.as_str(), l)).collect();
    let remote_by_id: HashMap<&str, &RemoteList> =
        remote_lists.iter().map(|l| (l.id.as_str(), l)).collect();
    let mut removed = HashSet::new();

    for (id, action) in plan(KIND_LIST, "", &local_times, &remote_times, tombs) {
        match action {
            SyncAction::Push => remote.create_list(local_by_id[id.as_str()]).await?,
            SyncAction::Pull => {
                let list = remote_by_id[id.as_str()];
                let (list_id, name, created_at) =
                    (list.id.clone(), list.name.clone(), list.created_at);
                pull_list(local, list_id, profile_id.to_string(), name, created_at).await?;
            }
            SyncAction::DeleteLocal => {
                removed.insert(id.clone());
                apply_local_deletion(local, KIND_LIST, id, tombs).await?;
            }
            SyncAction::DeleteRemote => {
                removed.insert(id.clone());
                remote.delete_list(&id).await?;
            }
            SyncAction::NoOp => {}
        }
    }
    Ok(removed)
}

async fn pull_list(
    local: &Arc<ProfileStore>,
    list_id: String,
    profile_id: String,
    name: String,
    created_at: i64,
) -> Result<()> {
    blocking(local, move |store| {
        store.upsert_list(&list_id, &profile_id, &name, created_at)
    })
    .await
}

async fn sync_list_items<R: SyncRemote>(
    local: &Arc<ProfileStore>,
    remote: &R,
    list_id: &str,
    local_items: &[ListItem],
    remote_items: &[RemoteListItem],
    tombs: &Tombstones,
) -> Result<()> {
    let local_times: HashMap<String, i64> = local_items
        .iter()
        .map(|i| (i.catalog_id.clone(), i.added_at))
        .collect();
    let remote_times: HashMap<String, i64> = remote_items
        .iter()
        .map(|i| (i.catalog_id.clone(), i.added_at))
        .collect();
    let local_by_id: HashMap<&str, &ListItem> = local_items
        .iter()
        .map(|i| (i.catalog_id.as_str(), i))
        .collect();
    let remote_by_id: HashMap<&str, &RemoteListItem> = remote_items
        .iter()
        .map(|i| (i.catalog_id.as_str(), i))
        .collect();
    let prefix = format!("{list_id}:");

    for (catalog_id, action) in plan(KIND_LIST_ITEM, &prefix, &local_times, &remote_times, tombs) {
        match action {
            SyncAction::Push => {
                remote
                    .add_list_item(local_by_id[catalog_id.as_str()])
                    .await?
            }
            SyncAction::Pull => {
                pull_list_item(local, list_id, remote_by_id[catalog_id.as_str()]).await?
            }
            SyncAction::DeleteLocal => {
                let key = list_item_key(list_id, &catalog_id);
                apply_local_deletion(local, KIND_LIST_ITEM, key, tombs).await?
            }
            SyncAction::DeleteRemote => remote.remove_list_item(list_id, &catalog_id).await?,
            SyncAction::NoOp => {}
        }
    }
    Ok(())
}

async fn pull_list_item(
    local: &Arc<ProfileStore>,
    list_id: &str,
    item: &RemoteListItem,
) -> Result<()> {
    let list_id = list_id.to_string();
    let item = item.clone();
    blocking(local, move |store| {
        store.upsert_list_item(
            &list_id,
            &item.catalog_id,
            &item.media_type,
            item.added_at,
            &remote_meta(&item),
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ---- resolve ----

    #[test]
    fn local_only_pushes() {
        assert_eq!(resolve(Some(100), None, None, None), SyncAction::Push);
    }

    #[test]
    fn remote_only_pulls() {
        assert_eq!(resolve(None, Some(100), None, None), SyncAction::Pull);
    }

    #[test]
    fn newer_copy_wins_and_equal_copies_need_no_write() {
        assert_eq!(resolve(Some(200), Some(100), None, None), SyncAction::Push);
        assert_eq!(resolve(Some(100), Some(200), None, None), SyncAction::Pull);
        assert_eq!(resolve(Some(100), Some(100), None, None), SyncAction::NoOp);
    }

    #[test]
    fn a_newer_remote_tombstone_deletes_the_local_copy() {
        assert_eq!(
            resolve(Some(100), None, None, Some(200)),
            SyncAction::DeleteLocal
        );
        assert_eq!(
            resolve(Some(100), None, None, Some(100)),
            SyncAction::DeleteLocal
        );
    }

    #[test]
    fn a_local_copy_newer_than_the_remote_tombstone_survives() {
        assert_eq!(resolve(Some(300), None, None, Some(200)), SyncAction::Push);
    }

    #[test]
    fn a_newer_local_tombstone_deletes_the_remote_copy() {
        assert_eq!(
            resolve(None, Some(100), Some(200), None),
            SyncAction::DeleteRemote
        );
    }

    #[test]
    fn a_remote_copy_newer_than_the_local_tombstone_returns() {
        assert_eq!(resolve(None, Some(300), Some(200), None), SyncAction::Pull);
    }

    #[test]
    fn nothing_on_either_side_is_a_no_op() {
        assert_eq!(resolve(None, None, Some(1), Some(2)), SyncAction::NoOp);
    }

    // ---- SyncGate ----

    #[test]
    fn the_gate_allows_one_sync_at_a_time() {
        let gate = SyncGate::new(Duration::ZERO);
        let permit = gate.try_start().expect("first sync starts");
        assert!(gate.try_start().is_none());
        drop(permit);
        assert!(gate.try_start().is_some());
    }

    #[test]
    fn the_gate_holds_back_a_sync_inside_the_minimum_gap() {
        let gate = SyncGate::new(Duration::from_secs(60));
        drop(gate.try_start().expect("first sync starts"));
        assert!(gate.try_start().is_none());
    }

    // ---- two devices against one in-memory gateway ----

    #[derive(Default)]
    struct FakeState {
        profiles: HashMap<String, RemoteProfile>,
        lists: HashMap<String, (String, RemoteList)>,
        statuses: HashMap<(String, String), RemoteWatchStatus>,
        deletions: HashMap<(String, String), i64>,
    }

    #[derive(Default)]
    struct FakeGateway {
        state: Mutex<FakeState>,
        clock: Mutex<i64>,
    }

    impl FakeGateway {
        fn tick(&self) -> i64 {
            let real = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let mut clock = self.clock.lock().unwrap();
            *clock = real.max(*clock + 1);
            *clock
        }
    }

    impl SyncRemote for FakeGateway {
        async fn deletions(&self) -> Result<Vec<RemoteDeletion>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .deletions
                .iter()
                .map(|((kind, key), at)| RemoteDeletion {
                    kind: kind.clone(),
                    key: key.clone(),
                    deleted_at: *at,
                })
                .collect())
        }
        async fn list_profiles(&self) -> Result<Vec<RemoteProfile>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .profiles
                .values()
                .cloned()
                .collect())
        }
        async fn push_profile(&self, p: &Profile) -> Result<()> {
            let mut s = self.state.lock().unwrap();
            s.deletions.remove(&(KIND_PROFILE.into(), p.id.clone()));
            s.profiles.insert(
                p.id.clone(),
                RemoteProfile {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    avatar_key: p.avatar_key.clone(),
                    created_at: p.created_at,
                    updated_at: p.updated_at,
                },
            );
            Ok(())
        }
        async fn delete_profile(&self, id: &str) -> Result<()> {
            let at = self.tick();
            let mut s = self.state.lock().unwrap();
            s.profiles.remove(id);
            s.lists.retain(|_, (profile, _)| profile != id);
            s.statuses.retain(|(profile, _), _| profile != id);
            s.deletions.insert((KIND_PROFILE.into(), id.into()), at);
            Ok(())
        }
        async fn list_lists(&self, profile_id: &str) -> Result<Vec<RemoteList>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .lists
                .values()
                .filter(|(profile, _)| profile == profile_id)
                .map(|(_, list)| list.clone())
                .collect())
        }
        async fn create_list(&self, list: &ListRecord) -> Result<()> {
            let mut s = self.state.lock().unwrap();
            let clash = s.lists.values().any(|(profile, other)| {
                *profile == list.profile_id
                    && other.id != list.id
                    && other.name.eq_ignore_ascii_case(&list.name)
            });
            if clash {
                return Err(anyhow!("409 duplicate list name"));
            }
            s.deletions.remove(&(KIND_LIST.into(), list.id.clone()));
            let entry = s.lists.entry(list.id.clone()).or_insert_with(|| {
                (
                    list.profile_id.clone(),
                    RemoteList {
                        id: list.id.clone(),
                        name: list.name.clone(),
                        created_at: list.created_at,
                        items: Vec::new(),
                    },
                )
            });
            entry.1.name = list.name.clone();
            Ok(())
        }
        async fn delete_list(&self, id: &str) -> Result<()> {
            let at = self.tick();
            let mut s = self.state.lock().unwrap();
            s.lists.remove(id);
            s.deletions.insert((KIND_LIST.into(), id.into()), at);
            Ok(())
        }
        async fn add_list_item(&self, item: &ListItem) -> Result<()> {
            let mut s = self.state.lock().unwrap();
            let key = list_item_key(&item.list_id, &item.catalog_id);
            s.deletions.remove(&(KIND_LIST_ITEM.into(), key));
            let (_, list) = s
                .lists
                .get_mut(&item.list_id)
                .ok_or_else(|| anyhow!("404 no list"))?;
            if !list.items.iter().any(|i| i.catalog_id == item.catalog_id) {
                list.items.push(RemoteListItem {
                    catalog_id: item.catalog_id.clone(),
                    media_type: item.media_type.clone(),
                    added_at: item.added_at,
                    title: item.title.clone(),
                    poster: item.poster.clone(),
                    year: item.year,
                    canonical_id: item.canonical_id.clone(),
                });
            }
            Ok(())
        }
        async fn remove_list_item(&self, list_id: &str, catalog_id: &str) -> Result<()> {
            let at = self.tick();
            let mut s = self.state.lock().unwrap();
            if let Some((_, list)) = s.lists.get_mut(list_id) {
                list.items.retain(|i| i.catalog_id != catalog_id);
            }
            let key = list_item_key(list_id, catalog_id);
            s.deletions.insert((KIND_LIST_ITEM.into(), key), at);
            Ok(())
        }
        async fn list_watch_status(&self, profile_id: &str) -> Result<Vec<RemoteWatchStatus>> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .statuses
                .iter()
                .filter(|((profile, _), _)| profile == profile_id)
                .map(|(_, status)| status.clone())
                .collect())
        }
        async fn set_watch_status(&self, st: &WatchStatus) -> Result<()> {
            let mut s = self.state.lock().unwrap();
            let key = watch_status_key(&st.profile_id, &st.catalog_id);
            s.deletions.remove(&(KIND_WATCH_STATUS.into(), key));
            s.statuses.insert(
                (st.profile_id.clone(), st.catalog_id.clone()),
                RemoteWatchStatus {
                    catalog_id: st.catalog_id.clone(),
                    status: st.status.as_str().into(),
                    episode_id: st.episode_id.clone(),
                    resume_seconds: st.resume_seconds,
                    updated_at: st.updated_at,
                    title: st.title.clone(),
                    poster: st.poster.clone(),
                    media_type: st.media_type.clone(),
                    year: st.year,
                    canonical_id: st.canonical_id.clone(),
                },
            );
            Ok(())
        }
        async fn delete_watch_status(&self, profile_id: &str, catalog_id: &str) -> Result<()> {
            let at = self.tick();
            let mut s = self.state.lock().unwrap();
            s.statuses
                .remove(&(profile_id.to_string(), catalog_id.to_string()));
            let key = watch_status_key(profile_id, catalog_id);
            s.deletions.insert((KIND_WATCH_STATUS.into(), key), at);
            Ok(())
        }
    }

    struct Device {
        store: Arc<ProfileStore>,
        _dir: tempfile::TempDir,
    }

    fn device() -> Device {
        let dir = tempfile::tempdir().unwrap();
        Device {
            store: Arc::new(ProfileStore::open(dir.path()).unwrap()),
            _dir: dir,
        }
    }

    const META: TitleMeta<'static> = TitleMeta {
        media_type: Some("movie"),
        title: Some("Heat"),
        poster: None,
        year: Some(1995),
        canonical_id: None,
    };

    async fn sync(device: &Device, gateway: &FakeGateway) {
        run_sync(&device.store, gateway).await.unwrap();
    }

    async fn pause() {
        // Local timestamps are milliseconds. A short pause keeps two writes apart.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    #[tokio::test]
    async fn a_removed_list_item_stays_removed_after_three_syncs() {
        let (a, b, gateway) = (device(), device(), FakeGateway::default());
        let profile = a.store.create_profile("Ada", "amber").unwrap();
        let list = a.store.create_list(&profile.id, "Weekend").unwrap();
        a.store
            .add_list_item(&list.id, "tt1", "movie", &META)
            .unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;
        assert_eq!(b.store.list_items(&list.id).unwrap().len(), 1);

        pause().await;
        a.store.remove_list_item(&list.id, "tt1").unwrap();
        for _ in 0..3 {
            sync(&a, &gateway).await;
            sync(&b, &gateway).await;
        }
        assert!(a.store.list_items(&list.id).unwrap().is_empty());
        assert!(b.store.list_items(&list.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_removed_status_stays_removed_after_three_syncs() {
        let (a, b, gateway) = (device(), device(), FakeGateway::default());
        let profile = a.store.create_profile("Ada", "amber").unwrap();
        a.store
            .set_watch_status(&profile.id, "tt1", WatchState::Watched, None, None, &META)
            .unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;
        assert!(b
            .store
            .get_watch_status(&profile.id, "tt1")
            .unwrap()
            .is_some());

        pause().await;
        b.store.remove_watch_status(&profile.id, "tt1").unwrap();
        for _ in 0..3 {
            sync(&b, &gateway).await;
            sync(&a, &gateway).await;
        }
        assert!(a
            .store
            .get_watch_status(&profile.id, "tt1")
            .unwrap()
            .is_none());
        assert!(b
            .store
            .get_watch_status(&profile.id, "tt1")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn an_item_added_again_after_a_removal_returns_everywhere() {
        let (a, b, gateway) = (device(), device(), FakeGateway::default());
        let profile = a.store.create_profile("Ada", "amber").unwrap();
        let list = a.store.create_list(&profile.id, "Weekend").unwrap();
        a.store
            .add_list_item(&list.id, "tt1", "movie", &META)
            .unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;

        pause().await;
        a.store.remove_list_item(&list.id, "tt1").unwrap();
        sync(&a, &gateway).await;
        pause().await;
        a.store
            .add_list_item(&list.id, "tt1", "movie", &META)
            .unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;
        sync(&a, &gateway).await;

        assert_eq!(a.store.list_items(&list.id).unwrap().len(), 1);
        assert_eq!(b.store.list_items(&list.id).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_deleted_list_and_a_deleted_profile_stay_deleted() {
        let (a, b, gateway) = (device(), device(), FakeGateway::default());
        let ada = a.store.create_profile("Ada", "amber").unwrap();
        let femi = a.store.create_profile("Femi", "rose").unwrap();
        let list = a.store.create_list(&ada.id, "Weekend").unwrap();
        a.store
            .add_list_item(&list.id, "tt1", "movie", &META)
            .unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;

        pause().await;
        b.store.delete_list(&list.id).unwrap();
        b.store.delete_profile(&femi.id).unwrap();
        for _ in 0..3 {
            sync(&b, &gateway).await;
            sync(&a, &gateway).await;
        }
        assert!(a.store.list_lists(&ada.id).unwrap().is_empty());
        assert_eq!(a.store.list_profiles().unwrap().len(), 1);
        assert_eq!(b.store.list_profiles().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn two_lists_with_one_name_merge_into_the_older_one() {
        let (a, b, gateway) = (device(), device(), FakeGateway::default());
        let profile = a.store.create_profile("Ada", "amber").unwrap();
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;

        let older = a.store.create_list(&profile.id, "Weekend").unwrap();
        a.store
            .add_list_item(&older.id, "tt1", "movie", &META)
            .unwrap();
        pause().await;
        let newer = b.store.create_list(&profile.id, "weekend").unwrap();
        b.store
            .add_list_item(&newer.id, "tt2", "movie", &META)
            .unwrap();

        sync(&a, &gateway).await;
        sync(&b, &gateway).await;
        sync(&a, &gateway).await;
        sync(&b, &gateway).await;

        for device in [&a, &b] {
            let lists = device.store.list_lists(&profile.id).unwrap();
            assert_eq!(lists.len(), 1, "one list must remain");
            assert_eq!(lists[0].id, older.id);
            assert_eq!(device.store.list_items(&older.id).unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn a_repeat_sync_with_no_change_writes_nothing() {
        let (a, gateway) = (device(), FakeGateway::default());
        let profile = a.store.create_profile("Ada", "amber").unwrap();
        let list = a.store.create_list(&profile.id, "Weekend").unwrap();
        a.store
            .add_list_item(&list.id, "tt1", "movie", &META)
            .unwrap();
        a.store
            .set_watch_status(&profile.id, "tt1", WatchState::ToWatch, None, None, &META)
            .unwrap();
        sync(&a, &gateway).await;
        let before = gateway.state.lock().unwrap().statuses.len();
        sync(&a, &gateway).await;
        sync(&a, &gateway).await;
        assert_eq!(gateway.state.lock().unwrap().statuses.len(), before);
        assert_eq!(a.store.list_items(&list.id).unwrap().len(), 1);
    }
}
