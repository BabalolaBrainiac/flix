use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Saved reading position for one publication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub publication_id: String,
    pub chapter_id: String,
    pub page: usize,
    pub updated_at: String, // ISO 8601 or simple timestamp
}

/// JSON file store for reading progress.
#[derive(Serialize, Deserialize, Default)]
pub struct ProgressStore {
    positions: HashMap<String, ReadingPosition>, // key: publication_id
    #[serde(skip)]
    file_path: PathBuf,
}

impl ProgressStore {
    pub fn load(data_dir: &Path) -> Result<Self> {
        let file_path = data_dir.join("reader").join("progress.json");
        if file_path.exists() {
            let data = std::fs::read_to_string(&file_path)
                .context("Failed to read reader progress file")?;
            let mut store: ProgressStore =
                serde_json::from_str(&data).context("Failed to parse reader progress file")?;
            store.file_path = file_path;
            Ok(store)
        } else {
            Ok(Self {
                positions: HashMap::new(),
                file_path,
            })
        }
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).context("Failed to create reader directory")?;
        }
        let tmp_path = self.file_path.with_extension("tmp");
        let data =
            serde_json::to_string_pretty(self).context("Failed to serialize reader progress")?;
        std::fs::write(&tmp_path, data).context("Failed to write reader progress file")?;
        std::fs::rename(&tmp_path, &self.file_path)
            .context("Failed to save reader progress file")?;
        Ok(())
    }

    pub fn get(&self, publication_id: &str) -> Option<&ReadingPosition> {
        self.positions.get(publication_id)
    }

    pub fn set(&mut self, position: ReadingPosition) {
        self.positions
            .insert(position.publication_id.clone(), position);
    }
}
