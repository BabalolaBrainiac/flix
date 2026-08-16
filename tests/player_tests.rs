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
}
