const PREFERRED_LANGUAGES: &[&str] = &["eng", "en", "english"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePolicy {
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
}

impl LanguagePolicy {
    pub fn anime() -> Self {
        Self {
            audio_languages: ["jpn", "ja", "japanese"].map(str::to_string).to_vec(),
            subtitle_languages: PREFERRED_LANGUAGES
                .iter()
                .map(|value| value.to_string())
                .collect(),
        }
    }
    /// Creates the standard playback policy.
    ///
    /// Audio: prefer English, then fall back to original audio and the file's default track.
    pub fn standard() -> Self {
        let audio_override = std::env::var("FLIX_AUDIO_LANGUAGE").ok().and_then(|value| {
            let parsed: Vec<String> = value
                .split(',')
                .map(|item| item.trim().to_ascii_lowercase())
                .filter(|item| !item.is_empty() && item.chars().all(|c| c.is_ascii_alphanumeric()))
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        });

        let english: Vec<String> = PREFERRED_LANGUAGES.iter().map(|&s| s.to_string()).collect();
        let mut default_audio = english.clone();
        default_audio.push("original".to_string());

        Self {
            audio_languages: audio_override.unwrap_or(default_audio),
            subtitle_languages: english,
        }
    }
}
