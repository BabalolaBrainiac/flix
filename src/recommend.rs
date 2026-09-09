use crate::stremio::CatalogItem;
use std::collections::HashSet;

/// Result of resolving free-text keywords into Cinemeta genre names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeywords {
    /// Cinemeta genre names (max 3).
    pub genres: Vec<String>,
    /// Tokens that didn't map to a genre.
    pub free_text: Vec<String>,
}

/// A catalog item paired with its computed relevance score.
#[derive(Debug, Clone)]
pub struct ScoredItem {
    pub item: CatalogItem,
    pub score: f64,
}

/// Filter criteria applied after scoring.
#[derive(Debug, Clone)]
pub struct RecommendFilter {
    pub min_rating: Option<f64>,
    pub since_year: Option<u32>,
    pub limit: usize,
}

/// Strategy for fetching anime catalog items based on genre support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimeCatalogStrategy {
    /// All genres are supported by Kitsu — use genre-filtered catalog.
    GenreFiltered { catalog_id: String, genre: String },
    /// Some genres unsupported — use trending and filter locally.
    TrendingWithLocalFilter,
    /// No genres specified — use popular.
    Popular,
}

const MAX_GENRES: usize = 3;

/// Genres supported by the Anime Kitsu addon.
const KITSU_GENRES: &[&str] = &[
    "Action",
    "Adventure",
    "Comedy",
    "Drama",
    "Sci-Fi",
    "Space",
    "Mystery",
    "Magic",
];

/// Returns true if the genre is supported by the Anime Kitsu addon.
pub fn kitsu_supports_genre(genre: &str) -> bool {
    KITSU_GENRES.contains(&genre)
}

/// Resolve free-text input into Cinemeta genre names and leftover tokens.
pub fn resolve_keywords(input: &str) -> ResolvedKeywords {
    let lowered = input.to_lowercase();
    let tokens: Vec<&str> = lowered
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();

    let mut genres: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut free_text: Vec<String> = Vec::new();

    let mut i = 0;
    while i < tokens.len() {
        // Try two-token phrases first.
        if i + 1 < tokens.len() {
            let pair = format!("{} {}", tokens[i], tokens[i + 1]);
            if let Some(mapped) = map_token(&pair) {
                for g in mapped {
                    if seen.insert(g.to_string()) && genres.len() < MAX_GENRES {
                        genres.push(g.to_string());
                    }
                }
                i += 2;
                continue;
            }
        }

        let token = tokens[i];
        if let Some(mapped) = map_token(token) {
            for g in mapped {
                if seen.insert(g.to_string()) && genres.len() < MAX_GENRES {
                    genres.push(g.to_string());
                }
            }
        } else {
            free_text.push(token.to_string());
        }
        i += 1;
    }

    ResolvedKeywords { genres, free_text }
}

/// Map a single token or two-token phrase to zero or more genre names.
fn map_token(token: &str) -> Option<&'static [&'static str]> {
    match token {
        "action" => Some(&["Action"]),
        "adventure" => Some(&["Adventure"]),
        "animation" | "animated" => Some(&["Animation"]),
        "biography" | "biopic" => Some(&["Biography"]),
        "comedy" => Some(&["Comedy"]),
        "crime" => Some(&["Crime"]),
        "crime mystery" => Some(&["Mystery"]),
        "detective" => Some(&["Mystery"]),
        "documentary" | "doc" => Some(&["Documentary"]),
        "drama" => Some(&["Drama"]),
        "family" => Some(&["Family"]),
        "fantasy" => Some(&["Fantasy"]),
        "game show" | "gameshow" => Some(&["Game-Show"]),
        "history" | "historical" => Some(&["History"]),
        "horror" => Some(&["Horror"]),
        "magic" => Some(&["Magic"]),
        "mystery" => Some(&["Mystery"]),
        "reality" | "reality tv" => Some(&["Reality-TV"]),
        "romance" => Some(&["Romance"]),
        "romcom" | "romantic comedy" => Some(&["Romance", "Comedy"]),
        "scifi" | "sci fi" | "sci-fi" => Some(&["Sci-Fi"]),
        "space" => Some(&["Space"]),
        "sport" | "sports" => Some(&["Sport"]),
        "talk show" | "talkshow" => Some(&["Talk-Show"]),
        "thriller" => Some(&["Thriller"]),
        "war" => Some(&["War"]),
        "western" => Some(&["Western"]),
        _ => None,
    }
}

/// Score and sort catalog items against resolved keywords.
pub fn score_items(items: Vec<CatalogItem>, resolved: &ResolvedKeywords) -> Vec<ScoredItem> {
    let mut scored: Vec<ScoredItem> = items
        .into_iter()
        .map(|item| {
            let score = compute_score(&item, resolved);
            ScoredItem { item, score }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ra = parse_rating(&a.item);
                let rb = parse_rating(&b.item);
                rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.item.name.cmp(&b.item.name))
    });

    scored
}

fn compute_score(item: &CatalogItem, resolved: &ResolvedKeywords) -> f64 {
    let genre_coverage = compute_genre_coverage(item, resolved);
    let rating = parse_rating(item).unwrap_or(0.5);
    let recency = compute_recency(item);
    let text_match = compute_text_match(item, resolved);

    genre_coverage * 0.4 + rating * 0.3 + recency + text_match
}

fn compute_genre_coverage(item: &CatalogItem, resolved: &ResolvedKeywords) -> f64 {
    if resolved.genres.is_empty() {
        return 0.5;
    }
    let item_genres: &[String] = item.genres.as_deref().unwrap_or(&[]);
    let matched = resolved
        .genres
        .iter()
        .filter(|g| item_genres.iter().any(|ig| ig == *g))
        .count();
    matched as f64 / resolved.genres.len() as f64
}

fn parse_rating(item: &CatalogItem) -> Option<f64> {
    item.imdb_rating.as_deref()?.parse::<f64>().ok()
}

fn compute_recency(item: &CatalogItem) -> f64 {
    let Some(year) = parse_year(item) else {
        return 0.0;
    };
    if year >= 2024 {
        0.2
    } else if year >= 2020 {
        0.15
    } else if year >= 2015 {
        0.1
    } else if year >= 2010 {
        0.05
    } else {
        0.0
    }
}

fn parse_year(item: &CatalogItem) -> Option<u32> {
    let info = item.release_info.as_deref()?;
    // Take the first 4 consecutive digits.
    let mut digits = String::new();
    for c in info.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            if digits.len() == 4 {
                return digits.parse().ok();
            }
        } else if !digits.is_empty() {
            break;
        }
    }
    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        None
    }
}

fn compute_text_match(item: &CatalogItem, resolved: &ResolvedKeywords) -> f64 {
    let name_lower = item.name.to_lowercase();
    let desc_lower = item.description.as_deref().unwrap_or("").to_lowercase();

    let mut score = 0.0_f64;
    for term in &resolved.free_text {
        if name_lower.contains(term.as_str()) || desc_lower.contains(term.as_str()) {
            score += 0.1;
            if score >= 0.3 {
                return 0.3;
            }
        }
    }
    score
}

/// Apply post-scoring filters: min rating, since year, dedup, limit.
pub fn apply_filter(scored: Vec<ScoredItem>, filter: &RecommendFilter) -> Vec<ScoredItem> {
    let mut seen_ids = HashSet::new();
    scored
        .into_iter()
        .filter(|si| {
            if let Some(min) = filter.min_rating {
                if let Some(rating) = parse_rating(&si.item) {
                    if rating < min {
                        return false;
                    }
                }
            }
            if let Some(since) = filter.since_year {
                if let Some(year) = parse_year(&si.item) {
                    if year < since {
                        return false;
                    }
                }
            }
            seen_ids.insert(si.item.id.clone())
        })
        .take(filter.limit)
        .collect()
}

/// Decide which anime catalog strategy to use based on genre support.
pub fn anime_catalog_for_genres(genres: &[String]) -> AnimeCatalogStrategy {
    if genres.is_empty() {
        return AnimeCatalogStrategy::Popular;
    }
    if genres.iter().all(|g| kitsu_supports_genre(g)) {
        AnimeCatalogStrategy::GenreFiltered {
            catalog_id: "kitsu-anime-popular".to_string(),
            genre: genres[0].clone(),
        }
    } else {
        AnimeCatalogStrategy::TrendingWithLocalFilter
    }
}
