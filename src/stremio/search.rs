use super::*;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchCatalog {
    Movie,
    Series,
    Anime,
}

impl StremioClient {
    pub async fn search_catalog(&self, query: &str, catalog: SearchCatalog) -> SearchResults {
        let started = Instant::now();
        let (name, result) = match catalog {
            SearchCatalog::Movie => (
                "Movies",
                self.catalog(
                    &self.cinemeta_base,
                    "movie",
                    "top",
                    query,
                    CatalogKind::Cinemeta,
                )
                .await,
            ),
            SearchCatalog::Series => (
                "Series",
                self.catalog(
                    &self.cinemeta_base,
                    "series",
                    "top",
                    query,
                    CatalogKind::Cinemeta,
                )
                .await,
            ),
            SearchCatalog::Anime => ("Anime", self.search_anime(query).await),
        };
        tracing::info!(
            catalog = name,
            elapsed_ms = started.elapsed().as_millis() as u64,
            success = result.is_ok(),
            "Catalog search completed"
        );
        result_from_requests(vec![(name, result)])
    }

    async fn search_anime(&self, query: &str) -> Result<Vec<CatalogItem>> {
        let request = self.catalog(
            &self.anime_base,
            "anime",
            "kitsu-anime-list",
            query,
            CatalogKind::AnimeKitsu,
        );
        match tokio::time::timeout(Duration::from_millis(2500), request).await {
            Ok(Ok(items)) if !items.is_empty() => return Ok(items),
            _ => {}
        }
        tracing::warn!("Anime catalog is unavailable or empty. Checking the general catalog.");
        let fallback = async {
            let (movies, series) = tokio::join!(
                self.catalog(
                    &self.cinemeta_base,
                    "movie",
                    "top",
                    query,
                    CatalogKind::Cinemeta
                ),
                self.catalog(
                    &self.cinemeta_base,
                    "series",
                    "top",
                    query,
                    CatalogKind::Cinemeta
                )
            );
            let mut failure = None;
            let mut items = Vec::new();
            for result in [movies, series] {
                match result {
                    Ok(found) => items.extend(found.into_iter().take(8)),
                    Err(error) => failure = Some(error),
                }
            }
            let checked: Vec<_> = stream::iter(items)
                .map(|mut item| async move {
                    let has_anime_genre = item.genres.as_ref().is_some_and(|genres| {
                        genres.iter().any(|g| g.eq_ignore_ascii_case("Anime"))
                    });
                    if has_anime_genre || self.content_is_anime(&item.media_type, &item.id).await? {
                        item.genres.get_or_insert_default().push("Anime".into());
                        Ok(Some(item))
                    } else {
                        Ok(None)
                    }
                })
                .buffered(6)
                .collect::<Vec<Result<Option<CatalogItem>>>>()
                .await;
            let mut found = Vec::new();
            for item in checked {
                match item {
                    Ok(Some(item)) => found.push(item),
                    Ok(None) => {}
                    Err(error) => failure = Some(error),
                }
            }
            if found.is_empty() {
                if let Some(error) = failure {
                    return Err(error);
                }
            }
            Ok(found)
        };
        tokio::time::timeout(Duration::from_millis(3500), fallback)
            .await
            .context("Anime search timed out. Try again.")?
    }
}
