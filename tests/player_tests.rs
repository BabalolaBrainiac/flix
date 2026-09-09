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
