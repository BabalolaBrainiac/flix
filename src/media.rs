use crate::meta::parse;
use crate::session::TorrentFile;
use std::ops::Range;

const WARMUP_HEAD_BYTES: u64 = 1_048_576;
const WARMUP_TAIL_BYTES: u64 = 512 * 1024;

// Larger warm-up for files above 2 GiB (typical 4K content).
// 4K video has higher bitrate and larger codec metadata.
const WARMUP_LARGE_THRESHOLD: u64 = 2 * 1_073_741_824;
const WARMUP_LARGE_HEAD_BYTES: u64 = 3 * 1_048_576;
const WARMUP_LARGE_TAIL_BYTES: u64 = 1_048_576;

pub fn select_default_video(files: &[TorrentFile]) -> Option<&TorrentFile> {
    let first_episode = files
        .iter()
        .filter(|file| file.is_video)
        .filter_map(|file| {
            let parsed = parse::parse(&file.name);
            Some((parsed.season?, parsed.episode?, file.index, file))
        })
        .min_by_key(|item| (item.0, item.1, item.2));
    if let Some((_, _, _, file)) = first_episode {
        return Some(file);
    }

    files
        .iter()
        .filter(|file| file.is_video)
        .max_by_key(|file| file.length)
}

pub fn ordered_videos(files: &[TorrentFile]) -> Vec<&TorrentFile> {
    let mut videos: Vec<_> = files.iter().filter(|file| file.is_video).collect();
    videos.sort_by(|left, right| {
        let left_episode = episode_key(&left.name);
        let right_episode = episode_key(&right.name);
        left_episode
            .cmp(&right_episode)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.index.cmp(&right.index))
    });
    videos
}

pub fn next_video_position(files: &[TorrentFile], selected_position: usize) -> Option<usize> {
    let selected = files.get(selected_position)?;
    let ordered = ordered_videos(files);
    let current = ordered
        .iter()
        .position(|file| file.index == selected.index)?;
    let next = ordered.get(current + 1)?;
    files.iter().position(|file| file.index == next.index)
}

fn episode_key(name: &str) -> (bool, u32, u32) {
    let parsed = parse::parse(name);
    match (parsed.season, parsed.episode) {
        (Some(season), Some(episode)) => (false, season, episode),
        _ => (true, u32::MAX, u32::MAX),
    }
}

pub fn startup_range(length: u64) -> Option<Range<u64>> {
    if length == 0 {
        return None;
    }

    let (head, _) = warmup_sizes(length);
    Some(0..length.min(head))
}

pub fn metadata_prefetch_range(length: u64) -> Option<Range<u64>> {
    let (head, tail) = warmup_sizes(length);
    if length > head + tail {
        Some(length - tail..length)
    } else {
        None
    }
}

pub fn warmup_ranges(length: u64) -> Vec<Range<u64>> {
    let Some(startup) = startup_range(length) else {
        return Vec::new();
    };
    match metadata_prefetch_range(length) {
        Some(metadata) => vec![startup, metadata],
        None => vec![startup],
    }
}

fn warmup_sizes(length: u64) -> (u64, u64) {
    if length >= WARMUP_LARGE_THRESHOLD {
        (WARMUP_LARGE_HEAD_BYTES, WARMUP_LARGE_TAIL_BYTES)
    } else {
        (WARMUP_HEAD_BYTES, WARMUP_TAIL_BYTES)
    }
}
