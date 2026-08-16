use flix::episode_queue::EpisodeQueue;
use flix::stremio::{CatalogItem, CatalogKind, Episode};

fn episode(id: &str, season: u32, number: u32) -> Episode {
    Episode {
        id: id.to_string(),
        title: Some(format!("Episode {number}")),
        season,
        episode: number,
    }
}

#[test]
fn advances_across_a_season_boundary() {
    let item = CatalogItem {
        id: "tt1234567".to_string(),
        media_type: "series".to_string(),
        name: "Show".to_string(),
        release_info: Some("2026".to_string()),
        kind: CatalogKind::Cinemeta,
    };
    let episodes = vec![
        episode("tt1234567:2:1", 2, 1),
        episode("tt1234567:1:2", 1, 2),
        episode("tt1234567:1:1", 1, 1),
    ];
    let mut queue = EpisodeQueue::new(item, episodes, "tt1234567:1:2").expect("episode queue");

    assert_eq!(queue.current().season, 1);
    assert_eq!(queue.current().episode, 2);
    assert_eq!(
        queue.next().map(|value| (value.season, value.episode)),
        Some((2, 1))
    );
    assert!(queue.advance());
    assert_eq!((queue.current().season, queue.current().episode), (2, 1));
    assert!(!queue.advance());
}

#[test]
fn builds_anime_subtitle_context_for_the_current_episode() {
    let item = CatalogItem {
        id: "kitsu:42".to_string(),
        media_type: "series".to_string(),
        name: "Anime".to_string(),
        release_info: Some("2026".to_string()),
        kind: CatalogKind::AnimeKitsu,
    };
    let queue = EpisodeQueue::new(item, vec![episode("kitsu:42:10", 2, 10)], "kitsu:42:10")
        .expect("episode queue");

    let context = queue
        .subtitle_context(queue.current())
        .expect("anime subtitle context");

    assert_eq!(context.series_id, "kitsu:42");
    assert_eq!(context.selected_episode, 10);
}
