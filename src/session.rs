use anyhow::{anyhow, Context, Result};
use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use tokio::time::{timeout_at, Instant};

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

pub struct TorrentSession {
    inner: Arc<Session>,
}

impl TorrentSession {
    pub async fn new(download_dir: &Path) -> Result<Self> {
        let session = Session::new_with_opts(
            download_dir.to_path_buf(),
            SessionOptions {
                disable_dht_persistence: false,
                ..Default::default()
            },
        )
        .await
        .context("Failed to create librqbit session")?;
        Ok(Self { inner: session })
    }

    pub async fn add(&self, src: Source) -> Result<TorrentId> {
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
            ..Default::default()
        };

        let deadline = Instant::now() + Duration::from_secs(45);
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

    pub async fn warm_stream(&self, id: TorrentId, file: usize) -> Result<()> {
        let (_, length) = self.open_stream(id, file)?;
        let ranges = crate::media::warmup_ranges(length);
        let warmup_secs = if length >= 2 * 1_073_741_824 { 45 } else { 20 };
        let warmup = async {
            for range in ranges {
                let (mut stream, _) = self.open_stream(id, file)?;
                stream.seek(std::io::SeekFrom::Start(range.start)).await?;
                read_region(&mut stream, range.end - range.start).await?;
            }
            Ok::<(), anyhow::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(warmup_secs), warmup)
            .await
            .context("Stream warm-up timed out")??;
        Ok(())
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
