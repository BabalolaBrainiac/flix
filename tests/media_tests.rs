use flix::media::{
    metadata_prefetch_range, next_video_position, ordered_videos, select_default_video,
    startup_range, warmup_ranges,
};
use flix::session::TorrentFile;

fn video(index: usize, name: &str, length: u64) -> TorrentFile {
    TorrentFile {
        index,
        name: name.to_string(),
        length,
        is_video: true,
    }
}

#[test]
fn season_pack_selects_the_first_episode() {
    let files = vec![
        video(7, "Show.S01E08.mkv", 900),
        video(0, "Show.S01E01.mkv", 700),
        video(1, "Show.S01E02.mkv", 1_000),
    ];

    assert_eq!(select_default_video(&files).map(|file| file.index), Some(0));
    assert_eq!(
        ordered_videos(&files)
            .into_iter()
            .map(|file| file.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 7]
    );
    assert_eq!(next_video_position(&files, 1), Some(2));
    assert_eq!(next_video_position(&files, 2), Some(0));
    assert_eq!(next_video_position(&files, 0), None);
}

#[test]
fn movie_torrent_selects_the_largest_video() {
    let files = vec![
        video(0, "sample.mkv", 100),
        video(1, "Open.Movie.2026.mkv", 1_000),
    ];

    assert_eq!(select_default_video(&files).map(|file| file.index), Some(1));
}

#[test]
fn warmup_ranges_stay_inside_the_file() {
    assert!(warmup_ranges(0).is_empty());
    assert_eq!(warmup_ranges(100), vec![0..100]);
    assert_eq!(
        warmup_ranges(10 * 1_048_576),
        vec![0..1_048_576, (10 * 1_048_576 - 512 * 1024)..10 * 1_048_576]
    );
}

#[test]
fn warmup_ranges_expand_for_large_files() {
    let four_gb = 4 * 1_073_741_824_u64;
    let ranges = warmup_ranges(four_gb);
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0], 0..3 * 1_048_576);
    assert_eq!(ranges[1], four_gb - 1_048_576..four_gb);
}

#[test]
fn startup_warmup_does_not_wait_for_optional_tail_metadata() {
    let length = 10 * 1_048_576;

    assert_eq!(startup_range(length), Some(0..1_048_576));
    assert_eq!(
        metadata_prefetch_range(length),
        Some((10 * 1_048_576 - 512 * 1024)..length)
    );
}
