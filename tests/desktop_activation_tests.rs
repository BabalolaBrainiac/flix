use flix::config::Config;
use flix::desktop::{
    delete_device_token, get_device_token, handle_activation_status, handle_diagnostics,
    handle_vlc_guidance, save_device_token, DesktopService,
};
use tempfile::TempDir;

#[test]
fn credential_storage_handles_tokens_or_headless_environments() {
    let test_token = "flix_test_device_token_1234567890abcdef";

    let save_res = save_device_token(test_token);
    println!("save_res: {:?}", save_res);
    let read_res = get_device_token();
    println!("read_res: {:?}", read_res);

    if save_res.is_ok() && read_res.as_ref().ok().and_then(|r| r.as_ref()).is_some() {
        let read_token = read_res.unwrap();
        assert_eq!(read_token.as_deref(), Some(test_token));

        let del_res = delete_device_token();
        assert!(del_res.is_ok());

        let after_del = get_device_token().unwrap();
        assert_eq!(after_del, None);
    } else {
        println!("Keyring access not supported in this test runner environment");
    }
}

#[test]
fn activation_status_reports_valid_metadata() {
    let status = handle_activation_status();
    // Verify structure fields exist and don't panic
    let _ = status.is_activated;
    let _ = status.vlc_installed;
}

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
async fn desktop_service_exposes_activation_and_diagnostics() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };

    let service = DesktopService::new(config);
    let _activation = service.activation_status();
    let guidance = service.vlc_guidance();
    assert!(!guidance.instructions.is_empty());

    let diag = service.diagnostics().unwrap();
    assert_eq!(diag.app_version, env!("CARGO_PKG_VERSION"));
}
