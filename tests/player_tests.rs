#[cfg(unix)]
mod unix {
    use flix::player::ManagedPlayer;
    use std::process::Command;

    #[test]
    fn managed_player_stops_its_child() {
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start test child");
        let id = child.id();
        let mut player = ManagedPlayer::new(child);

        player.stop().expect("stop child");

        assert!(player.has_exited().expect("read child state"));
        let status = Command::new("kill")
            .args(["-0", &id.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("check child");
        assert!(!status.success());
    }

    #[tokio::test]
    async fn startup_reports_an_immediate_exit_code() {
        let child = Command::new("sh").args(["-c", "exit 7"]).spawn().unwrap();
        let mut player = ManagedPlayer::new(child);
        let error = player.wait_for_startup().await.unwrap_err();
        let exit = &error
            .downcast_ref::<flix::player::PlayerStartupError>()
            .unwrap()
            .0;
        assert_eq!(exit.exit_code, Some(7));
        assert!(!exit.success);
        assert_eq!(exit.signal, None);
    }

    #[tokio::test]
    async fn startup_does_not_accept_an_immediate_successful_exit() {
        let child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
        let mut player = ManagedPlayer::new(child);
        assert!(player.wait_for_startup().await.is_err());
    }

    #[tokio::test]
    async fn startup_accepts_a_running_process() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let mut player = ManagedPlayer::new(child);
        player.wait_for_startup().await.unwrap();
        assert!(player.poll_exit().unwrap().is_none());
        player.stop().unwrap();
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_vlc_does_not_receive_unsupported_instance_options() {
    use flix::player::{command, PlaybackOptions, Player, PlayerKind};
    let player = Player {
        kind: PlayerKind::Vlc,
        path: "vlc".into(),
        version: None,
    };
    let command = command(&player, "test.mkv", &PlaybackOptions::new("Test".into(), 1));
    assert!(!command
        .get_args()
        .any(|arg| arg.to_string_lossy().contains("one-instance")));
}

#[test]
fn verified_anime_tracks_override_english_preferences() {
    use flix::player::{command, PlaybackOptions, Player, PlayerKind};
    let options = PlaybackOptions::new("Language fixture".into(), 1).with_selected_tracks(Some(
        flix::playback::anime::SelectedTracks {
            audio: 1,
            subtitle: Some(1),
        },
    ));
    for kind in [PlayerKind::Vlc, PlayerKind::Mpv] {
        let player = Player {
            kind,
            path: "test-player".into(),
            version: None,
        };
        let command = command(&player, "sample.mkv", &options);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        let expected = match kind {
            PlayerKind::Vlc => [
                "--audio-language=jpn,ja,japanese",
                "--audio-track=1",
                "--sub-track=1",
            ],
            PlayerKind::Mpv => ["--alang=jpn,ja,japanese", "--aid=2", "--sid=2"],
        };
        for argument in expected {
            assert!(args.iter().any(|arg| arg == argument));
        }
    }
}

#[test]
fn macos_focus_command_targets_correct_player_applications() {
    use flix::player::{macos_focus_command_args, PlayerKind};

    let (program_vlc, args_vlc) = macos_focus_command_args(PlayerKind::Vlc);
    assert_eq!(program_vlc, "/usr/bin/open");
    assert_eq!(args_vlc, vec!["-a", "VLC"]);

    let (program_mpv, args_mpv) = macos_focus_command_args(PlayerKind::Mpv);
    assert_eq!(program_mpv, "/usr/bin/open");
    assert_eq!(args_mpv, vec!["-a", "mpv"]);
}

#[test]
fn windows_focus_command_targets_process_identifier() {
    use flix::player::windows_focus_command_args;

    let (program, args) = windows_focus_command_args(54321);
    assert_eq!(program, "powershell");
    assert!(args.contains(&"-NoProfile".to_string()));
    assert!(args.contains(&"-NonInteractive".to_string()));
    assert!(args.contains(&"-WindowStyle".to_string()));
    assert!(args.contains(&"Hidden".to_string()));

    let cmd_idx = args
        .iter()
        .position(|a| a == "-Command")
        .expect("has -Command");
    let script = &args[cmd_idx + 1];
    assert!(script.contains("AppActivate(54321)"));
}
