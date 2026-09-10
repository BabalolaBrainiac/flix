use super::*;

impl StremioClient {
    /// Uses content metadata independently of the selected catalog.
    pub async fn content_is_anime(&self, media_type: &str, id: &str) -> Result<bool> {
        if !valid_content_id(id) {
            return Err(anyhow!("Content identifier is invalid"));
        }
        if id.starts_with("kitsu:") {
            return Ok(true);
        }
        let catalog_id = id.split(':').next().unwrap_or(id);
        if !catalog_id.starts_with("tt") {
            return Ok(false);
        }
        let media_type = if id.contains(':') {
            "series"
        } else {
            media_type
        };
        let url = resource_url(&self.cinemeta_base, "meta", media_type, catalog_id)?;
        let value = self.get_json(url, "metadata").await?;
        let meta = value
            .get("meta")
            .filter(|meta| meta.is_object())
            .context("Content metadata is unavailable. Try again.")?;
        Ok(metadata_is_anime(meta))
    }
}

fn metadata_is_anime(meta: &Value) -> bool {
    let genres = meta.get("genres").or_else(|| meta.get("genre"));
    if has_label(genres, &["anime"]) {
        return true;
    }
    has_label(genres, &["animation"]) && has_label(meta.get("country"), &["japan", "jp", "jpn"])
}

fn has_label(value: Option<&Value>, labels: &[&str]) -> bool {
    match value {
        Some(Value::Array(values)) => values.iter().any(|value| has_label(Some(value), labels)),
        Some(Value::String(value)) => value.split([',', '/', ';']).any(|value| {
            labels
                .iter()
                .any(|label| value.trim().eq_ignore_ascii_case(label))
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identifies_anime_without_changing_catalog_identifiers() {
        assert!(metadata_is_anime(
            &json!({"genres":["Animation", "Action"], "country":"Japan"})
        ));
        assert!(metadata_is_anime(
            &json!({"genre":["Animation"], "country":["US", "Japan"]})
        ));
        assert!(metadata_is_anime(&json!({"genres":["Anime"]})));
        assert!(!metadata_is_anime(
            &json!({"genres":["Animation"], "country":"USA"})
        ));
        assert!(!metadata_is_anime(
            &json!({"genres":["Drama"], "country":"Japan"})
        ));
        assert!(!metadata_is_anime(&json!({})));
    }
}
