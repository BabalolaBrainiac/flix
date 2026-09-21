use flix::recommend::{
    anime_catalog_for_genres, apply_filter, resolve_keywords, score_items, AnimeCatalogStrategy,
    RecommendFilter, RecommendPool,
};
use flix::stremio::{CatalogItem, CatalogKind};

fn make_item(
    id: &str,
    name: &str,
    genres: Option<Vec<&str>>,
    rating: Option<&str>,
    year: Option<&str>,
) -> CatalogItem {
    CatalogItem {
        id: id.to_string(),
        media_type: "movie".to_string(),
        name: name.to_string(),
        release_info: year.map(|y| y.to_string()),
        poster: None,
        genres: genres.map(|gs| gs.into_iter().map(|g| g.to_string()).collect()),
        imdb_rating: rating.map(|r| r.to_string()),
        description: None,
        kind: CatalogKind::Cinemeta,
    }
}

#[test]
fn keyword_maps_to_correct_genre() {
    let resolved = resolve_keywords("action");
    assert!(resolved.genres.contains(&"Action".to_string()));
    assert!(resolved.free_text.is_empty());
}

#[test]
fn synonym_maps_to_correct_genre() {
    let resolved = resolve_keywords("scifi");
    assert!(resolved.genres.contains(&"Sci-Fi".to_string()));

    let resolved = resolve_keywords("detective");
    assert!(resolved.genres.contains(&"Mystery".to_string()));

    let resolved = resolve_keywords("romcom");
    assert!(resolved.genres.contains(&"Romance".to_string()));
    assert!(resolved.genres.contains(&"Comedy".to_string()));
}

#[test]
fn unknown_token_stays_free_text() {
    let resolved = resolve_keywords("xyzabc");
    assert!(resolved.genres.is_empty());
    assert!(resolved.free_text.contains(&"xyzabc".to_string()));
}

#[test]
fn genre_count_caps_at_3() {
    let resolved = resolve_keywords("action comedy drama thriller horror");
    assert_eq!(resolved.genres.len(), 3);
}

#[test]
fn item_matching_two_genres_ranks_above_one() {
    let item_both = make_item(
        "tt1",
        "Both Genres",
        Some(vec!["Action", "Mystery"]),
        Some("7.0"),
        Some("2020"),
    );
    let item_one = make_item(
        "tt2",
        "One Genre",
        Some(vec!["Action"]),
        Some("7.0"),
        Some("2020"),
    );

    let resolved = resolve_keywords("action mystery");
    let scored = score_items(vec![item_one, item_both], &resolved);

    assert_eq!(scored[0].item.id, "tt1");
    assert!(scored[0].score > scored[1].score);
}

#[test]
fn min_rating_removes_low_rated_item() {
    let high = make_item("tt1", "High", None, Some("8.0"), Some("2020"));
    let low = make_item("tt2", "Low", None, Some("6.5"), Some("2020"));

    let resolved = resolve_keywords("");
    let scored = score_items(vec![high, low], &resolved);

    let filter = RecommendFilter {
        min_rating: Some(7.0),
        since_year: None,
        offset: 0,
        limit: 10,
    };
    let filtered = apply_filter(scored, &filter);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].item.id, "tt1");
}

#[test]
fn no_duplicate_ids_in_result() {
    let first = make_item("tt1", "First", None, Some("8.0"), Some("2020"));
    let dupe = make_item("tt1", "Duplicate", None, Some("7.0"), Some("2019"));

    let resolved = resolve_keywords("");
    let scored = score_items(vec![first, dupe], &resolved);

    let filter = RecommendFilter {
        min_rating: None,
        since_year: None,
        offset: 0,
        limit: 10,
    };
    let filtered = apply_filter(scored, &filter);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].item.name, "First");
}

#[test]
fn unsupported_kitsu_genre_selects_trending() {
    let strategy = anime_catalog_for_genres(&["Horror".to_string()]);
    assert_eq!(strategy, AnimeCatalogStrategy::TrendingWithLocalFilter);
}

#[test]
fn supported_kitsu_genre_uses_filtered() {
    let strategy = anime_catalog_for_genres(&["Action".to_string()]);
    assert_eq!(
        strategy,
        AnimeCatalogStrategy::GenreFiltered {
            catalog_id: "kitsu-anime-popular".to_string(),
            genre: "Action".to_string(),
        }
    );
}

#[test]
fn empty_genres_uses_popular() {
    let strategy = anime_catalog_for_genres(&[]);
    assert_eq!(strategy, AnimeCatalogStrategy::Popular);
}

fn scored(ids: &[&str]) -> Vec<flix::recommend::ScoredItem> {
    let items = ids
        .iter()
        .map(|id| make_item(id, id, None, Some("8.0"), Some("2020")))
        .collect();
    score_items(items, &resolve_keywords(""))
}

#[test]
fn offset_and_limit_cut_consecutive_pages_with_no_repeat() {
    let pool = scored(&["a", "b", "c", "d", "e"]);
    let page = |offset, limit| {
        let filter = RecommendFilter {
            min_rating: None,
            since_year: None,
            offset,
            limit,
        };
        apply_filter(pool.clone(), &filter)
            .into_iter()
            .map(|s| s.item.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(page(0, 2), ["a", "b"]);
    assert_eq!(page(2, 2), ["c", "d"]);
    assert_eq!(page(4, 2), ["e"]);
    assert!(page(6, 2).is_empty());
}

#[test]
fn a_new_round_extends_the_pool_without_moving_earlier_items() {
    let mut pool = RecommendPool::default();
    assert!(pool.can_grow());
    pool.absorb(scored(&["a", "b"]));
    pool.absorb(scored(&["b", "c"]));
    let ids: Vec<_> = pool.items.iter().map(|s| s.item.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "c"]);
    assert_eq!(pool.rounds, 2);
    assert!(!pool.exhausted);
}

#[test]
fn a_round_with_no_new_item_ends_the_pool() {
    let mut pool = RecommendPool::default();
    pool.absorb(scored(&["a"]));
    pool.absorb(scored(&["a"]));
    assert!(pool.exhausted);
    assert!(!pool.can_grow());
}

#[test]
fn a_pool_stops_growing_at_the_round_limit() {
    let mut pool = RecommendPool::default();
    for round in 0..flix::recommend::MAX_ROUNDS {
        pool.absorb(scored(&[&format!("id{round}")]));
    }
    assert!(!pool.can_grow());
}
