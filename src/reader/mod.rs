pub mod cache;
pub mod mangadex;
pub mod progress;
pub mod source;

use serde::{Deserialize, Serialize};

/// A manga or comic publication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub year: Option<u32>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub cover_url: Option<String>,
    pub source: ReaderSource,
}

/// A chapter of a publication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub chapter: Option<String>, // e.g. "1", "2.5"
    pub volume: Option<String>,
    pub title: Option<String>,
    pub language: String,
    pub pages: usize,
    pub external_url: Option<String>, // if set, chapter is NOT on MangaDex
}

/// A set of page image URLs for a chapter.
#[derive(Debug, Clone)]
pub struct PageSet {
    pub chapter_id: String,
    pub page_urls: Vec<String>,
}

/// Which upstream source provided this content.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReaderSource {
    MangaDex,
}

impl std::fmt::Display for ReaderSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReaderSource::MangaDex => write!(f, "mangadex"),
        }
    }
}
