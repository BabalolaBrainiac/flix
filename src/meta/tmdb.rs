use super::{ParsedName, TmdbInfo};
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct TmdbSearchResponse {
    results: Option<Vec<TmdbSearchResult>>,
}

#[derive(Deserialize)]
struct TmdbSearchResult {
    id: u32,
    poster_path: Option<String>,
    vote_average: Option<f32>,
}

pub async fn search(name: &ParsedName) -> Result<Option<TmdbInfo>> {
    // TMDB public API key can be passed via env or config
    let api_key = match std::env::var("TMDB_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return Ok(None),
    };

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(std::time::Duration::from_secs(5))
        .min_tls_version(reqwest::tls::Version::TLS_1_3)
        .build()?;

    let mut query_params = vec![
        ("api_key", api_key.as_str()),
        ("query", name.title.as_str()),
    ];
    let year_str;
    if let Some(year) = name.year {
        year_str = year.to_string();
        query_params.push(("year", &year_str));
    }

    let url = "https://api.themoviedb.org/3/search/movie";
    let resp = match client.get(url).query(&query_params).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(None),
    };

    let search_res: TmdbSearchResponse = match resp.json().await {
        Ok(j) => j,
        Err(_) => return Ok(None),
    };

    if let Some(results) = search_res.results {
        if let Some(first) = results.into_iter().next() {
            return Ok(Some(TmdbInfo {
                tmdb_id: first.id,
                poster_path: first.poster_path,
                vote_average: first.vote_average,
            }));
        }
    }

    Ok(None)
}
