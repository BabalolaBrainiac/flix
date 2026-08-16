use crate::session::{TorrentFile, TorrentId, TorrentSession};
use crate::subtitles;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct PlaybackPreparation {
    pub subtitle: Option<PathBuf>,
    pub notes: Vec<String>,
}

pub struct InteractivePlaybackPreparation {
    pub cached_subtitle: Option<PathBuf>,
    pub subtitle_results: Vec<subtitles::SubtitleResult>,
    pub notes: Vec<String>,
}

pub async fn prepare_interactive(
    session: &TorrentSession,
    data_dir: &Path,
    id: TorrentId,
    file: &TorrentFile,
    stremio_context: Option<&crate::stremio::SubtitleContext>,
) -> InteractivePlaybackPreparation {
    let info_hash = session.info_hash(id);
    let warmup = session.warm_stream(id, file.index);
    let subtitle = async {
        let hash = info_hash?;
        tokio::time::timeout(
            Duration::from_secs(25),
            subtitles::english_subtitle_options(data_dir, &hash, file.index, &file.name),
        )
        .await
        .map_err(|_| anyhow::anyhow!("OpenSubtitles search timed out"))?
    };
    let stremio_subtitle = async {
        let Some(context) = stremio_context else {
            return Ok(None);
        };
        let hash = session.info_hash(id)?;
        tokio::time::timeout(
            Duration::from_secs(25),
            crate::stremio::find_anime_subtitle(data_dir, &hash, file.index, &file.name, context),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Stremio subtitle lookup timed out"))?
    };
    let (warmup_result, subtitle_result, stremio_result) =
        tokio::join!(warmup, subtitle, stremio_subtitle);
    let mut notes = Vec::new();
    if let Err(error) = warmup_result {
        notes.push(format!("Stream warm-up was incomplete: {error}"));
    }
    let (open_subtitles_cache, subtitle_results) = match subtitle_result {
        Ok(options) => {
            if options.cached.is_none() && options.results.is_empty() && !subtitles::is_configured()
            {
                notes.push("OpenSubtitles is not configured.".to_string());
            }
            (options.cached, options.results)
        }
        Err(error) => {
            notes.push(format!("English subtitle search failed: {error}"));
            (None, Vec::new())
        }
    };
    let stremio_subtitle = match stremio_result {
        Ok(path) => path,
        Err(error) => {
            notes.push(format!("Stremio subtitle lookup failed: {error}"));
            None
        }
    };
    InteractivePlaybackPreparation {
        cached_subtitle: stremio_subtitle.or(open_subtitles_cache),
        subtitle_results,
        notes,
    }
}

pub async fn prepare(
    session: &TorrentSession,
    data_dir: &Path,
    id: TorrentId,
    file: &TorrentFile,
) -> PlaybackPreparation {
    let info_hash = session.info_hash(id);
    let warmup = session.warm_stream(id, file.index);
    let subtitle = async {
        let hash = info_hash?;
        tokio::time::timeout(
            Duration::from_secs(25),
            subtitles::find_english_subtitle(data_dir, &hash, file.index, &file.name),
        )
        .await
        .map_err(|_| anyhow::anyhow!("OpenSubtitles lookup timed out"))?
    };
    let (warmup_result, subtitle_result) = tokio::join!(warmup, subtitle);
    let mut notes = Vec::new();
    if let Err(error) = warmup_result {
        notes.push(format!("Stream warm-up was incomplete: {error}"));
    }
    let subtitle = match subtitle_result {
        Ok(path) => {
            if path.is_none() && !subtitles::is_configured() {
                notes.push("OpenSubtitles is not configured.".to_string());
            }
            path
        }
        Err(error) => {
            notes.push(format!("English subtitle lookup failed: {error}"));
            None
        }
    };
    PlaybackPreparation { subtitle, notes }
}
