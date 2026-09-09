use anyhow::{anyhow, Context, Result};
use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, PeerConnectionOptions,
    Session, SessionOptions,
};
use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio::time::{timeout, timeout_at, Instant};

pub type TorrentId = usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Magnet(String),
    Url(String),
    File(PathBuf),
}

impl Source {
    pub fn parse(input: &str) -> Result<Self> {
        let value = input.trim();
        if value.is_empty() {
            return Err(anyhow!("Torrent source cannot be empty"));
        }

        if value.starts_with("magnet:") {
            validate_magnet(value)?;
            return Ok(Self::Magnet(value.to_string()));
        }

        if value.starts_with("http://") || value.starts_with("https://") {
            reqwest::Url::parse(value).context("Invalid torrent URL")?;
            return Ok(Self::Url(value.to_string()));
        }

        if value.contains("://") {
            return Err(anyhow!("Unsupported torrent URL scheme"));
        }

        let path = PathBuf::from(value);
        let is_torrent = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"));
        if !is_torrent {
            return Err(anyhow!(
                "Local torrent source must use the .torrent extension"
            ));
        }

        Ok(Self::File(path))
    }
}

fn validate_magnet(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("Invalid magnet URL")?;
    let info_hash = url
        .query_pairs()
        .find_map(|(key, value)| (key == "xt").then_some(value))
        .filter(|value| value.starts_with("urn:btih:"))
        .context("Magnet URL has no BitTorrent info hash")?;
    let hash = info_hash.trim_start_matches("urn:btih:");
    let valid_hex = hash.len() == 40 && hash.chars().all(|character| character.is_ascii_hexdigit());
    let valid_base32 = hash.len() == 32
        && hash
            .chars()
            .all(|character| character.is_ascii_alphanumeric());
    if !valid_hex && !valid_base32 {
        return Err(anyhow!("Magnet URL has an invalid BitTorrent info hash"));
    }
    Ok(())
}

#[derive(Clone)]
pub struct TorrentFile {
    pub index: usize,
    pub name: String,
    pub length: u64,
    pub is_video: bool,
}

pub struct Progress {
    pub total: u64,
    pub done: u64,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub peers: usize,
    pub finished: bool,
}

#[derive(Clone)]
pub struct TorrentSession {
    inner: Arc<Session>,
    _lifetime: Arc<tokio_util::sync::DropGuard>,
}

// librqbit buffers deferred writes in memory. This cap keeps peer downloads
// off the stream-read path without allowing unbounded process memory growth.
const DEFERRED_WRITE_BUFFER_MIB: usize = 16;
const METADATA_PREFETCH_TIMEOUT: Duration = Duration::from_secs(5);

// librqbit keeps 128 peer connection attempts open and waits 10 seconds for
// each one. A public swarm lists hundreds of addresses, and most do not answer,
// so the default holds every slot on a dead address for the first 20 seconds of
// a session. A measured cold start needed 21 seconds before the first byte
// arrived. A shorter timeout frees each slot sooner and reaches a live peer
// earlier.
const PEER_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);

// Warm-up limits. A fresh session must connect to the swarm before any byte can
// arrive, so the first limit is generous. After data starts, a stall is the
// signal that the source is dead, not the clock.
const WARMUP_CONNECT_GRACE: Duration = Duration::from_secs(45);
const WARMUP_STALL_WINDOW: Duration = Duration::from_secs(20);
const WARMUP_CEILING: Duration = Duration::from_secs(120);
const WARMUP_SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Swarm state at the end of a warm-up attempt.
///
/// The caller shows these counters, so a failure says why the source did not
/// start. A stream returns no byte until a complete piece passes its hash
/// check, so `piece_length` explains a long wait on a torrent with big pieces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WarmupReport {
    pub live_peers: usize,
    pub fetched_bytes: u64,
    pub checked_bytes: u64,
    pub download_bps: u64,
    pub piece_length: u64,
}

impl std::fmt::Display for WarmupReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} live peers, {:.2} MiB received, {:.2} MiB verified, {:.2} MiB/s, {:.2} MiB pieces",
            self.live_peers,
            mib(self.fetched_bytes),
            mib(self.checked_bytes),
            mib(self.download_bps),
            mib(self.piece_length),
        )
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

impl TorrentSession {
    pub async fn new(download_dir: &Path) -> Result<Self> {
        let session = Session::new_with_opts(
            download_dir.to_path_buf(),
            SessionOptions {
                // Persistent DHT pins a fixed UDP port. A second session, such
                // as the web client while the CLI runs, or two plays in a row,
                // then fails to bind that port with "address already in use".
                // Each playback uses a fresh temporary session, so persistence
                // adds no value. Disable it to get an ephemeral DHT port and
                // let sessions coexist.
                disable_dht_persistence: true,
                defer_writes_up_to: Some(DEFERRED_WRITE_BUFFER_MIB),
                peer_opts: Some(PeerConnectionOptions {
                    connect_timeout: Some(PEER_CONNECT_TIMEOUT),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .with_context(|| {
            format!(
                "Failed to create librqbit session. Download directory: {}",
                download_dir.display()
            )
        })?;
        let lifetime = Arc::new(session.cancellation_token().clone().drop_guard());
        Ok(Self {
            inner: session,
            _lifetime: lifetime,
        })
    }

    pub fn cancel(&self) {
        self.inner.cancellation_token().cancel();
    }

    pub async fn shutdown(&self) {
        self.cancel();
        // librqbit 8.1.1 moves file handles during stop(), before queued writes finish.
        // Cancel the tasks and keep their storage alive until their references are released.
        tokio::task::yield_now().await;
    }

    pub async fn add(&self, src: Source) -> Result<TorrentId> {
        self.add_once(src, None).await
    }

    /// Adds a torrent and selects one file at add time.
    ///
    /// A second file selection on a live torrent calls `update_only_files` in
    /// librqbit. That call holds the torrent write lock while it locks the peer
    /// map, and a peer task locks them in the opposite order, so the pair can
    /// deadlock. It also leaves the torrent with nothing selected for a moment,
    /// and librqbit disconnects a peer that connects during that window.
    /// Selecting the file at add time avoids both problems.
    ///
    /// The file index comes from a catalog, so it can be wrong for the torrent.
    /// The add then runs again without a selection. The second element of the
    /// result reports whether the selection was applied.
    pub async fn add_with_file(&self, src: Source, file_index: usize) -> Result<(TorrentId, bool)> {
        match self.add_once(src.clone(), Some(vec![file_index])).await {
            Ok(id) => Ok((id, true)),
            Err(error) if is_invalid_file_selection(&error) => {
                tracing::warn!(
                    "Could not add the torrent with file {file_index} selected: {error:#}"
                );
                Ok((self.add_once(src, None).await?, false))
            }
            Err(error) => Err(error),
        }
    }

    async fn add_once(&self, src: Source, only_files: Option<Vec<usize>>) -> Result<TorrentId> {
        let add_torrent = match src {
            Source::Magnet(value) | Source::Url(value) => AddTorrent::from_url(value),
            Source::File(f) => {
                let path_str = f.to_str().context("Invalid filename UTF-8")?;
                AddTorrent::from_local_filename(path_str)
                    .context("Failed to load .torrent file from local path")?
            }
        };

        let options = AddTorrentOptions {
            overwrite: true,
            only_files,
            ..Default::default()
        };

        // Each playback uses a fresh session, so the DHT starts cold and needs
        // time to bootstrap and find peers for a low-seed torrent. 90 seconds
        // gives a marginal source a fair chance before the app reports no peers.
        let deadline = Instant::now() + Duration::from_secs(90);
        let response = timeout_at(deadline, self.inner.add_torrent(add_torrent, Some(options)))
            .await
            .context("Timed out adding torrent (no peers or metadata source reachable)")??;
        let (id, handle) = match response {
            AddTorrentResponse::Added(id, handle)
            | AddTorrentResponse::AlreadyManaged(id, handle) => Ok((id, handle)),
            AddTorrentResponse::ListOnly(_) => Err(anyhow!("ListOnly response, expected Added")),
        }?;
        match timeout_at(deadline, handle.wait_until_initialized()).await {
            Ok(Ok(())) => Ok(id),
            Ok(Err(error)) => {
                let _ = self.inner.delete(TorrentIdOrHash::Id(id), false).await;
                Err(error).context("Failed to initialize torrent metadata")
            }
            Err(error) => {
                let _ = self.inner.delete(TorrentIdOrHash::Id(id), false).await;
                Err(error)
                    .context("Timed out waiting for torrent metadata (no seeds or peers reachable)")
            }
        }
    }

    pub fn handle(&self, id: TorrentId) -> Option<Arc<ManagedTorrent>> {
        self.inner.get(TorrentIdOrHash::Id(id))
    }

    pub fn files(&self, id: TorrentId) -> Result<Vec<TorrentFile>> {
        let handle = self.handle(id).context("Torrent not found")?;
        let guard = handle.metadata.load();
        let metadata = guard.as_ref().context("No metadata yet")?;
        let mut files = Vec::new();

        for (index, file_info) in metadata.file_infos.iter().enumerate() {
            let name = file_info.relative_filename.to_string_lossy().into_owned();
            let is_video =
                name.ends_with(".mp4") || name.ends_with(".mkv") || name.ends_with(".avi");
            files.push(TorrentFile {
                index,
                name,
                length: file_info.len,
                is_video,
            });
        }

        Ok(files)
    }

    pub fn info_hash(&self, id: TorrentId) -> Result<String> {
        let handle = self.handle(id).context("Torrent not found")?;
        Ok(handle.info_hash().as_string())
    }

    pub fn progress(&self, id: TorrentId) -> Option<Progress> {
        let handle = self.handle(id)?;
        let stats = handle.stats();

        let (dl_bps, up_bps, peers) = if let Some(ref live) = stats.live {
            (
                (live.download_speed.mbps * 1_048_576.0) as u64,
                (live.upload_speed.mbps * 1_048_576.0) as u64,
                live.snapshot.peer_stats.live,
            )
        } else {
            (0, 0, 0)
        };

        Some(Progress {
            total: stats.total_bytes,
            done: stats.progress_bytes,
            download_bps: dl_bps,
            upload_bps: up_bps,
            peers,
            finished: stats.finished,
        })
    }

    pub fn list(&self) -> Vec<TorrentId> {
        self.inner
            .with_torrents(|torrents| torrents.map(|(id, _)| id).collect())
    }

    pub fn open_stream(
        &self,
        id: TorrentId,
        file: usize,
    ) -> Result<(
        impl tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin + Send + Sync + 'static,
        u64,
    )> {
        let handle = self.handle(id).context("Torrent not found")?;
        let stream = handle.stream(file)?;
        let len = stream.len();
        Ok((stream, len))
    }

    /// Reads the start of the file so the player can begin without a stall.
    ///
    /// A fixed deadline cannot judge a source. A cold session needs 20 to 30
    /// seconds to reach a live peer, and the stream returns no byte until a
    /// complete piece passes its hash check, so the real cost follows the piece
    /// length and the swarm speed, not the file size. This warm-up therefore
    /// watches progress: it waits `WARMUP_CONNECT_GRACE` for the first byte, it
    /// stops when no new byte arrives for `WARMUP_STALL_WINDOW`, and it never
    /// runs longer than `WARMUP_CEILING`.
    pub async fn warm_stream(&self, id: TorrentId, file: usize) -> Result<WarmupReport> {
        let (_, length) = self.open_stream(id, file)?;
        let Some(range) = crate::media::startup_range(length) else {
            return Ok(self.warmup_report(id));
        };

        let read = self.read_range(id, file, range);
        tokio::pin!(read);
        let started = Instant::now();
        let mut samples = tokio::time::interval(WARMUP_SAMPLE_INTERVAL);
        samples.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut received = 0_u64;
        let mut last_change = started;

        loop {
            tokio::select! {
                // Check the read first. A finished read must win over a limit
                // that expires in the same moment.
                biased;
                result = &mut read => {
                    result?;
                    self.prefetch_metadata(id, file, length);
                    return Ok(self.warmup_report(id));
                }
                _ = samples.tick() => {
                    let report = self.warmup_report(id);
                    if report.fetched_bytes > received {
                        received = report.fetched_bytes;
                        last_change = Instant::now();
                    }
                    let waited = started.elapsed();
                    if let Some(reason) =
                        warmup_verdict(received, waited, last_change.elapsed())
                    {
                        return Err(warmup_error(reason, report, waited));
                    }
                }
            }
        }
    }

    /// Returns the swarm counters for one torrent.
    fn warmup_report(&self, id: TorrentId) -> WarmupReport {
        let Some(handle) = self.handle(id) else {
            return WarmupReport::default();
        };
        let stats = handle.stats();
        let (live_peers, download_bps, fetched_bytes) = match stats.live.as_ref() {
            Some(live) => (
                live.snapshot.peer_stats.live,
                (live.download_speed.mbps * 1_048_576.0) as u64,
                live.snapshot.fetched_bytes,
            ),
            None => (0, 0, 0),
        };
        WarmupReport {
            live_peers,
            fetched_bytes,
            checked_bytes: stats.progress_bytes,
            download_bps,
            piece_length: self.piece_length(id),
        }
    }

    /// Returns the piece length, or zero when the metadata is not loaded.
    fn piece_length(&self, id: TorrentId) -> u64 {
        let Some(handle) = self.handle(id) else {
            return 0;
        };
        let guard = handle.metadata.load();
        guard
            .as_ref()
            .map(|metadata| u64::from(metadata.lengths.default_piece_length()))
            .unwrap_or(0)
    }

    async fn read_range(&self, id: TorrentId, file: usize, range: Range<u64>) -> Result<()> {
        let (mut stream, _) = self.open_stream(id, file)?;
        stream.seek(std::io::SeekFrom::Start(range.start)).await?;
        read_region(&mut stream, range.end - range.start).await
    }

    fn prefetch_metadata(&self, id: TorrentId, file: usize, length: u64) {
        let Some(range) = crate::media::metadata_prefetch_range(length) else {
            return;
        };
        let session = self.clone();
        tokio::spawn(async move {
            match timeout(
                METADATA_PREFETCH_TIMEOUT,
                session.read_range(id, file, range),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::debug!(error = %error, "stream metadata prefetch failed")
                }
                Err(_) => tracing::debug!("stream metadata prefetch timed out"),
            }
        });
    }

    pub async fn select_files(&self, id: TorrentId, files: &[usize]) -> Result<()> {
        let handle = self.handle(id).context("Torrent not found")?;
        let selected: HashSet<_> = files.iter().copied().collect();
        self.inner
            .update_only_files(&handle, &selected)
            .await
            .context("Failed to update the torrent file selection")
    }

    pub async fn remove(&self, id: TorrentId, delete_files: bool) -> Result<()> {
        self.inner
            .delete(TorrentIdOrHash::Id(id), delete_files)
            .await
            .context("Failed to delete torrent")
    }

    pub async fn pause(&self, id: TorrentId) -> Result<()> {
        let handle = self.handle(id).context("Torrent not found")?;
        self.inner
            .pause(&handle)
            .await
            .context("Failed to pause torrent")
    }
}

/// Decides whether warm-up stops, from the progress counters alone.
///
/// `received` is the byte count that the swarm delivered. `waited` is the time
/// since warm-up started. `quiet` is the time since the byte count last grew.
/// A `Some` result holds the reason to stop.
fn warmup_verdict(received: u64, waited: Duration, quiet: Duration) -> Option<&'static str> {
    if received == 0 && waited >= WARMUP_CONNECT_GRACE {
        return Some("no peer sent data");
    }
    if received > 0 && quiet >= WARMUP_STALL_WINDOW {
        return Some("the download stalled");
    }
    if waited >= WARMUP_CEILING {
        return Some("the source is too slow");
    }
    None
}

/// Reports whether the torrent still needs a file selection after the add.
///
/// The add can apply the selection already. A second selection is necessary
/// only when the add did not apply one, or when the chosen file differs.
pub fn needs_file_selection(preferred: Option<usize>, preselected: bool, target: usize) -> bool {
    preferred.filter(|_| preselected) != Some(target)
}

fn warmup_error(reason: &str, report: WarmupReport, waited: Duration) -> anyhow::Error {
    anyhow!("{reason} after {}s ({report})", waited.as_secs())
}

async fn read_region(
    stream: &mut (impl AsyncRead + AsyncSeek + Unpin),
    mut remaining: u64,
) -> Result<()> {
    let mut buffer = vec![0_u8; 256 * 1024];
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let read = stream.read(&mut buffer[..limit]).await?;
        if read == 0 {
            return Err(anyhow!("Torrent stream ended during warm-up"));
        }
        remaining -= read as u64;
    }
    Ok(())
}

fn is_invalid_file_selection(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let message = cause.to_string();
        message.starts_with("file id ") && message.ends_with(" is out of range")
    })
}

#[cfg(test)]
mod tests {
    use super::{
        needs_file_selection, warmup_verdict, WarmupReport, WARMUP_CEILING, WARMUP_CONNECT_GRACE,
        WARMUP_STALL_WINDOW,
    };
    use std::time::Duration;

    const ZERO: Duration = Duration::from_secs(0);

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_releases_a_live_session_and_its_storage() {
        use super::*;
        use librqbit::{create_torrent, CreateTorrentOptions};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.mkv");
        std::fs::write(&path, vec![42u8; 128 * 1024]).unwrap();
        let torrent = create_torrent(&path, CreateTorrentOptions::default())
            .await
            .unwrap();
        let inner = Session::new_with_opts(
            directory.path().to_owned(),
            SessionOptions {
                disable_dht: true,
                defer_writes_up_to: Some(DEFERRED_WRITE_BUFFER_MIB),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let handle = inner
            .add_torrent(
                AddTorrent::from_bytes(torrent.as_bytes().unwrap()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        handle.wait_until_initialized().await.unwrap();
        let live = Arc::downgrade(&handle.live().unwrap());
        let weak = Arc::downgrade(&inner);
        let session = TorrentSession {
            _lifetime: Arc::new(inner.cancellation_token().clone().drop_guard()),
            inner,
        };
        session.shutdown().await;
        drop(handle);
        drop(session);
        tokio::time::timeout(Duration::from_secs(3), async {
            while weak.strong_count() > 0 || live.strong_count() > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Session tasks must release the torrent storage");
        directory
            .close()
            .expect("Torrent files must close before cache removal");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_releases_storage_during_a_peer_download() {
        use super::*;
        use librqbit::{create_torrent, CreateTorrentOptions};
        use std::io::Write;

        let seed_dir = tempfile::tempdir().unwrap();
        let download_dir = tempfile::tempdir().unwrap();
        let path = seed_dir.path().join("sample.mkv");
        let mut file = std::fs::File::create(&path).unwrap();
        let chunk = vec![42u8; 1024 * 1024];
        for _ in 0..32 {
            file.write_all(&chunk).unwrap();
        }
        drop(file);
        let torrent = create_torrent(
            &path,
            CreateTorrentOptions {
                piece_length: Some(256 * 1024),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .as_bytes()
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let seed = Session::new_with_opts(
            seed_dir.path().to_owned(),
            SessionOptions {
                disable_dht: true,
                listen_port_range: Some(address.port()..address.port() + 1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let seed_handle = seed
            .add_torrent(
                AddTorrent::from_bytes(torrent.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        seed_handle.wait_until_initialized().await.unwrap();

        let inner = Session::new_with_opts(
            download_dir.path().to_owned(),
            SessionOptions {
                disable_dht: true,
                defer_writes_up_to: Some(DEFERRED_WRITE_BUFFER_MIB),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let handle = inner
            .add_torrent(
                AddTorrent::from_bytes(torrent),
                Some(AddTorrentOptions {
                    initial_peers: Some(vec![address]),
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        handle.wait_until_initialized().await.unwrap();
        let live = handle.live().unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while live.stats_snapshot().fetched_bytes == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("The local peer must send data");
        assert!(live.stats_snapshot().fetched_bytes < 32 * 1024 * 1024);
        let weak_live = Arc::downgrade(&live);
        let weak_inner = Arc::downgrade(&inner);
        let session = TorrentSession {
            _lifetime: Arc::new(inner.cancellation_token().clone().drop_guard()),
            inner,
        };
        session.shutdown().await;
        drop(live);
        drop(handle);
        drop(session);
        tokio::time::timeout(Duration::from_secs(3), async {
            while weak_live.strong_count() > 0 || weak_inner.strong_count() > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Queued writes must release the cancelled session");
        download_dir
            .close()
            .expect("The download cache must be removable");
        seed.cancellation_token().cancel();
    }

    #[test]
    fn warm_up_waits_for_the_first_byte_until_the_connect_grace_ends() {
        let almost = WARMUP_CONNECT_GRACE - Duration::from_secs(1);

        assert_eq!(warmup_verdict(0, almost, almost), None);
        assert_eq!(
            warmup_verdict(0, WARMUP_CONNECT_GRACE, WARMUP_CONNECT_GRACE),
            Some("no peer sent data")
        );
    }

    #[test]
    fn warm_up_continues_past_the_connect_grace_while_bytes_arrive() {
        let waited = WARMUP_CONNECT_GRACE + Duration::from_secs(30);

        assert_eq!(warmup_verdict(4 * 1_048_576, waited, ZERO), None);
    }

    #[test]
    fn warm_up_stops_when_the_byte_count_stops_growing() {
        let waited = Duration::from_secs(60);

        assert_eq!(
            warmup_verdict(1_048_576, waited, WARMUP_STALL_WINDOW),
            Some("the download stalled")
        );
    }

    #[test]
    fn warm_up_stops_at_the_ceiling_even_with_progress() {
        assert_eq!(
            warmup_verdict(1_048_576, WARMUP_CEILING, ZERO),
            Some("the source is too slow")
        );
    }

    #[test]
    fn a_preselected_file_needs_no_second_selection() {
        assert!(!needs_file_selection(Some(7), true, 7));
        assert!(needs_file_selection(Some(7), true, 8));
        assert!(needs_file_selection(Some(7), false, 7));
        assert!(needs_file_selection(None, false, 0));
    }

    #[test]
    fn the_warm_up_report_shows_the_swarm_counters() {
        let report = WarmupReport {
            live_peers: 3,
            fetched_bytes: 10 * 1_048_576,
            checked_bytes: 4 * 1_048_576,
            download_bps: 1_048_576 / 2,
            piece_length: 4 * 1_048_576,
        };

        let text = report.to_string();

        assert!(text.contains("3 live peers"), "{text}");
        assert!(text.contains("10.00 MiB received"), "{text}");
        assert!(text.contains("4.00 MiB verified"), "{text}");
        assert!(text.contains("0.50 MiB/s"), "{text}");
        assert!(text.contains("4.00 MiB pieces"), "{text}");
    }
}
