const SUBTITLE_LANGUAGES: &[&str] = &["eng", "en", "english"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguagePolicy {
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
}

impl LanguagePolicy {
    /// Creates the standard playback policy.
    ///
    /// Audio: no forced language, so the player uses the file's default audio
    /// track. That track is the original language of the content, for example
    /// Japanese for anime or Korean for a Korean film. `FLIX_AUDIO_LANGUAGE`
    /// overrides this for a user who wants to force a language such as an
    /// English dub.
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

        let english: Vec<String> = SUBTITLE_LANGUAGES.iter().map(|&s| s.to_string()).collect();

        Self {
            // Empty means the player keeps the file's default audio track.
            audio_languages: audio_override.unwrap_or_default(),
            subtitle_languages: english,
        }
    }
}
