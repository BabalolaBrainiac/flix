use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::session::Source;

#[derive(Serialize, Deserialize, Clone)]
pub struct FilmMeta {
    pub title: String,
    pub year: Option<u32>,
    #[serde(default)]
    pub tmdb_id: Option<u32>,
    pub poster_path: Option<String>,
    pub tmdb_score: Option<f32>,
    pub letterboxd_score: Option<f32>,
    #[serde(default)]
    pub cached_at: Option<SystemTime>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SourceSerde {
    Magnet(String),
    Url(String),
    File(PathBuf),
}

impl From<&Source> for SourceSerde {
    fn from(s: &Source) -> Self {
        match s {
            Source::Magnet(m) => SourceSerde::Magnet(m.clone()),
            Source::Url(url) => SourceSerde::Url(url.clone()),
            Source::File(f) => SourceSerde::File(f.clone()),
        }
    }
}

impl From<&SourceSerde> for Source {
    fn from(s: &SourceSerde) -> Self {
        match s {
            SourceSerde::Magnet(m) => Source::Magnet(m.clone()),
            SourceSerde::Url(url) => Source::Url(url.clone()),
            SourceSerde::File(f) => Source::File(f.clone()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub info_hash: String,
    pub display_name: String,
    pub source: SourceSerde,
    pub added_at: SystemTime,
    pub last_played: Option<SystemTime>,
    pub meta: Option<FilmMeta>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    AddedDesc,
    TitleAsc,
    RatingDesc,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Library {
    entries: Vec<Entry>,
    #[serde(skip)]
    file_path: PathBuf,
}

impl Library {
    pub fn load(dir: &Path) -> Result<Self> {
        let file_path = dir.join("library.json");
        if file_path.exists() {
            let data = fs::read_to_string(&file_path)?;
            let mut lib: Library = serde_json::from_str(&data)?;
            lib.file_path = file_path;
            Ok(lib)
        } else {
            Ok(Self {
                entries: Vec::new(),
                file_path,
            })
        }
    }

    pub fn save(&self) -> Result<()> {
        let tmp_path = self.file_path.with_extension("tmp");
        let file = File::create(&tmp_path).context("Failed to create temporary library file")?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &self)?;
        writer
            .flush()
            .context("Failed to flush temporary library file")?;
        writer
            .get_ref()
            .sync_all()
            .context("Failed to sync temporary library file")?;
        fs::rename(&tmp_path, &self.file_path).context("Failed to overwrite library file")?;
        Ok(())
    }

    pub fn upsert(&mut self, e: Entry) {
        if let Some(existing) = self.entries.iter_mut().find(|x| x.info_hash == e.info_hash) {
            *existing = e;
        } else {
            self.entries.push(e);
        }
    }

    pub fn mark_played(&mut self, info_hash: &str) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.info_hash == info_hash)
        {
            entry.last_played = Some(SystemTime::now());
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn sorted(&self, by: Sort) -> Vec<&Entry> {
        let mut out: Vec<&Entry> = self.entries.iter().collect();
        match by {
            Sort::AddedDesc => {
                out.sort_by_key(|b| std::cmp::Reverse(b.added_at));
            }
            Sort::TitleAsc => {
                out.sort_by_key(|b| &b.display_name);
            }
            Sort::RatingDesc => {
                out.sort_by(|a, b| {
                    let a_score = a.meta.as_ref().and_then(|m| m.tmdb_score).unwrap_or(0.0);
                    let b_score = b.meta.as_ref().and_then(|m| m.tmdb_score).unwrap_or(0.0);
                    b_score
                        .partial_cmp(&a_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
        out
    }
}
