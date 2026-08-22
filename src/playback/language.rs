const PREFERRED_LANGUAGES: &[&str] = &["eng", "en", "english"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePolicy {
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
}

impl LanguagePolicy {
    /// Creates the standard playback policy.
    ///
    /// Audio: prefer English, then fall back to the file's default track. This
    /// keeps English content in English and lets an original-language-only file,
    /// such as anime with Japanese audio, fall back to that track instead of a
    /// dub the release happened to order first. `FLIX_AUDIO_LANGUAGE` overrides
    /// the audio preference, for example `jpn,ja` to force Japanese.
    ///
    /// Subtitles: prefer English.
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

        Self {
            audio_languages: audio_override.unwrap_or_else(|| english.clone()),
            subtitle_languages: english,
        }
    }
}
