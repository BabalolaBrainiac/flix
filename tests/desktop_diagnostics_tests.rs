use flix::config::Config;
use flix::desktop::{handle_diagnostics, handle_vlc_guidance, DesktopService};
use tempfile::TempDir;

#[test]
fn vlc_guidance_returns_guided_instructions_and_official_url() {
    let guidance = handle_vlc_guidance();
    assert!(!guidance.platform.is_empty());
    assert!(guidance.download_url.contains("videolan.org"));
    assert!(!guidance.instructions.is_empty());

    #[cfg(target_os = "windows")]
    {
        assert_eq!(guidance.platform, "windows");
        assert_eq!(
            guidance.command.as_deref(),
            Some("winget install VideoLAN.VLC")
        );
    }

    #[cfg(target_os = "macos")]
    {
        assert_eq!(guidance.platform, "macos");
        assert!(guidance.command.is_none());
        assert!(guidance.instructions.iter().any(|i| i.contains("DMG")));
    }
}

#[test]
fn diagnostics_report_excludes_secrets_and_contains_paths() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };

    let report = handle_diagnostics(&config, false).unwrap();
    assert_eq!(report.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        report.download_dir,
        config.download_dir.display().to_string()
    );
    assert_eq!(report.data_dir, config.data_dir.display().to_string());
    assert!(!report.active_session);

    let json = serde_json::to_string(&report).unwrap();
    // Confirm report contains no secrets, tokens, or hashes
    assert!(!json.contains("token_hash"));
    assert!(!json.contains("code_hash"));
    assert!(!json.contains("ADMIN_SECRET"));
    assert!(!json.contains("OPENSUBTITLES_PASSWORD"));
}

#[tokio::test]
async fn desktop_service_exposes_diagnostics() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };

    let service = DesktopService::new(config).unwrap();
    let guidance = service.vlc_guidance();
    assert!(!guidance.instructions.is_empty());

    let diag = service.diagnostics().unwrap();
    assert_eq!(diag.app_version, env!("CARGO_PKG_VERSION"));
}
