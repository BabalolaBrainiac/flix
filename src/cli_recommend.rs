use crate::cli_search::{self, SearchAction, SearchSelection};
use crate::recommend::{self, AnimeCatalogStrategy, RecommendFilter};
use crate::stremio::{CatalogItem, CatalogKind, StremioClient};
use anyhow::Result;

pub struct RecommendOptions {
    pub keywords: String,
    pub show: bool,
    pub movie: bool,
    pub anime: bool,
    pub limit: usize,
    pub min_rating: Option<f64>,
    pub since_year: Option<u32>,
    pub page: usize,
    pub json: bool,
    pub action_override: Option<SearchAction>,
}

struct FetchTask {
    media_type: String,
    catalog_id: String,
    extra: String,
    kind: CatalogKind,
}

pub async fn run(options: RecommendOptions) -> Result<Option<SearchSelection>> {
    let resolved = recommend::resolve_keywords(&options.keywords);

    // Determine which types to search
    let search_all = !options.show && !options.movie && !options.anime;
    let mut types: Vec<(&str, CatalogKind)> = Vec::new();
    if search_all || options.movie {
        types.push(("movie", CatalogKind::Cinemeta));
    }
    if search_all || options.show {
        types.push(("series", CatalogKind::Cinemeta));
    }
    if search_all || options.anime {
        types.push(("anime", CatalogKind::AnimeKitsu));
    }

    let client = StremioClient::from_env()?;
    let mut all_items: Vec<CatalogItem> = Vec::new();

    let skip = (options.page - 1) * 50; // Cinemeta returns 50 per page

    // Build fetch tasks
    let mut fetch_tasks: Vec<FetchTask> = Vec::new();

    for (media_type, kind) in &types {
        match kind {
            CatalogKind::Cinemeta => {
                if resolved.genres.is_empty() {
                    // No genres resolved — use a sensible default
                    let extra = if skip > 0 {
                        format!("genre=Action&skip={skip}")
                    } else {
                        "genre=Action".to_string()
                    };
                    fetch_tasks.push(FetchTask {
                        media_type: media_type.to_string(),
                        catalog_id: "top".to_string(),
                        extra,
                        kind: *kind,
                    });
                } else {
                    for genre in &resolved.genres {
                        let extra = if skip > 0 {
                            format!("genre={genre}&skip={skip}")
                        } else {
                            format!("genre={genre}")
                        };
                        fetch_tasks.push(FetchTask {
                            media_type: media_type.to_string(),
                            catalog_id: "top".to_string(),
                            extra,
                            kind: *kind,
                        });
                    }
                }
            }
            CatalogKind::AnimeKitsu => {
                match recommend::anime_catalog_for_genres(&resolved.genres) {
                    AnimeCatalogStrategy::GenreFiltered { catalog_id, genre } => {
                        fetch_tasks.push(FetchTask {
                            media_type: media_type.to_string(),
                            catalog_id,
                            extra: format!("genre={genre}"),
                            kind: *kind,
                        });
                    }
                    AnimeCatalogStrategy::TrendingWithLocalFilter => {
                        // Fetch trending, filter locally later via scoring
                        fetch_tasks.push(FetchTask {
                            media_type: media_type.to_string(),
                            catalog_id: "kitsu-anime-trending".to_string(),
                            extra: "genre=Action".to_string(),
                            kind: *kind,
                        });
                    }
                    AnimeCatalogStrategy::Popular => {
                        fetch_tasks.push(FetchTask {
                            media_type: media_type.to_string(),
                            catalog_id: "kitsu-anime-popular".to_string(),
                            extra: "genre=Action".to_string(),
                            kind: *kind,
                        });
                    }
                }
            }
        }
    }

    // Fetch all catalogs sequentially
    for task in &fetch_tasks {
        match client
            .discover(&task.media_type, &task.catalog_id, &task.extra, task.kind)
            .await
        {
            Ok(items) => all_items.extend(items),
            Err(e) => eprintln!("Catalog fetch failed for {}: {e}", task.media_type),
        }
    }

    if all_items.is_empty() {
        println!("No recommendations found for \"{}\".", options.keywords);
        return Ok(None);
    }

    // Score and filter
    let scored = recommend::score_items(all_items, &resolved);
    let filter = RecommendFilter {
        min_rating: options.min_rating,
        since_year: options.since_year,
        limit: options.limit,
    };
    let filtered = recommend::apply_filter(scored, &filter);

    if filtered.is_empty() {
        println!("No recommendations match the filters.");
        return Ok(None);
    }

    if options.json {
        let items: Vec<serde_json::Value> = filtered
            .iter()
            .map(|scored| {
                serde_json::json!({
                    "id": scored.item.id,
                    "name": scored.item.name,
                    "media_type": scored.item.media_type,
                    "release_info": scored.item.release_info,
                    "poster": scored.item.poster,
                    "genres": scored.item.genres,
                    "imdb_rating": scored.item.imdb_rating,
                    "description": scored.item.description,
                    "score": scored.score,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(None);
    }

    // Interactive selection — reuse the stage machine
    let items: Vec<CatalogItem> = filtered.into_iter().map(|s| s.item).collect();
    cli_search::select_from_items(&items, &client, options.action_override).await
}
