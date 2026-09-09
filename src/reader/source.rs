use super::mangadex::MangaDexClient;
use super::{Chapter, PageSet, Publication, ReaderSource};
use anyhow::Result;

pub enum ReaderClient {
    MangaDex(MangaDexClient),
}

impl ReaderClient {
    pub fn new(source: ReaderSource) -> Result<Self> {
        match source {
            ReaderSource::MangaDex => Ok(Self::MangaDex(MangaDexClient::new()?)),
        }
    }

    pub fn source(&self) -> ReaderSource {
        match self {
            Self::MangaDex(_) => ReaderSource::MangaDex,
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Publication>> {
        match self {
            Self::MangaDex(client) => client.search(query, limit).await,
        }
    }

    pub async fn chapters(
        &self,
        manga_id: &str,
        language: &str,
    ) -> Result<(Vec<Chapter>, Vec<String>)> {
        match self {
            Self::MangaDex(client) => client.chapters(manga_id, language).await,
        }
    }

    pub async fn pages(&self, chapter_id: &str) -> Result<PageSet> {
        match self {
            Self::MangaDex(client) => client.pages(chapter_id).await,
        }
    }

    pub async fn download_page(&self, url: &str) -> Result<Vec<u8>> {
        match self {
            Self::MangaDex(client) => client.download_page(url).await,
        }
    }
}
