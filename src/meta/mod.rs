pub mod letterboxd;
pub mod parse;
pub mod tmdb;

#[derive(Debug, Clone)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct TmdbInfo {
    pub tmdb_id: u32,
    pub poster_path: Option<String>,
    pub vote_average: Option<f32>,
}

pub async fn lookup(input: &str) -> crate::library::FilmMeta {
    let parsed = parse::parse(input);
    let (tmdb, letterboxd) = tokio::join!(
        tmdb::search(&parsed),
        letterboxd::search_score(&parsed.title, parsed.year)
    );
    let tmdb = tmdb.ok().flatten();
    let letterboxd = letterboxd.ok().flatten();
    crate::library::FilmMeta {
        title: parsed.title,
        year: parsed.year,
        tmdb_id: tmdb.as_ref().map(|item| item.tmdb_id),
        poster_path: tmdb.as_ref().and_then(|item| item.poster_path.clone()),
        tmdb_score: tmdb.and_then(|item| item.vote_average),
        letterboxd_score: letterboxd,
        cached_at: Some(std::time::SystemTime::now()),
    }
}
