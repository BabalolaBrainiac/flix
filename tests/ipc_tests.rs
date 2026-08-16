#[cfg(unix)]
mod unix {
    use flix::player::ipc::MpvIpc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn reads_playback_position_from_mpv_socket() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let socket = directory.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).expect("bind socket");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept client");
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request).await.expect("read request");
            assert!(request.contains("time-pos"));
            stream
                .get_mut()
                .write_all(b"{\"data\":42.5,\"request_id\":1,\"error\":\"success\"}\n")
                .await
                .expect("write response");
        });

        let position = MpvIpc::new(socket)
            .get_playback_position()
            .await
            .expect("read position");
        server.await.expect("server task");

        assert_eq!(position, Some(42.5));
    }

    #[tokio::test]
    async fn limits_ipc_response_wait() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let socket = directory.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).expect("bind socket");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept client");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });

        let started = std::time::Instant::now();
        let result = MpvIpc::new(socket).get_playback_position().await;
        server.abort();

        assert!(result.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn removes_the_ipc_socket_during_cleanup() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let socket = directory.path().join("mpv.sock");
        std::fs::write(&socket, b"test").expect("temporary socket marker");

        MpvIpc::new(socket.clone())
            .cleanup()
            .expect("cleanup socket");

        assert!(!socket.exists());
    }
}
