use crate::stremio::{CatalogItem, CatalogKind, Episode, SubtitleContext};
use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct EpisodeQueue {
    item: CatalogItem,
    episodes: Vec<Episode>,
    position: usize,
}

impl EpisodeQueue {
    pub fn new(item: CatalogItem, mut episodes: Vec<Episode>, selected_id: &str) -> Result<Self> {
        episodes.sort_by_key(|episode| (episode.season, episode.episode));
        let position = episodes
            .iter()
            .position(|episode| episode.id == selected_id)
            .ok_or_else(|| anyhow!("The selected episode is not in the episode queue"))?;
        Ok(Self {
            item,
            episodes,
            position,
        })
    }

    pub fn current(&self) -> &Episode {
        &self.episodes[self.position]
    }

    pub fn next(&self) -> Option<&Episode> {
        self.episodes.get(self.position + 1)
    }

    pub fn has_next(&self) -> bool {
        self.next().is_some()
    }

    pub fn advance(&mut self) -> bool {
        if self.next().is_none() {
            return false;
        }
        self.position += 1;
        true
    }

    pub fn select(&mut self, season: u32, episode_number: u32) -> bool {
        let Some(position) = self
            .episodes
            .iter()
            .position(|episode| episode.season == season && episode.episode == episode_number)
        else {
            return false;
        };
        self.position = position;
        true
    }

    pub fn subtitle_context(&self, episode: &Episode) -> Option<SubtitleContext> {
        (self.item.kind == CatalogKind::AnimeKitsu).then(|| SubtitleContext {
            series_id: self.item.id.clone(),
            selected_episode: episode.episode,
        })
    }
}
