use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
struct IpcCommand<'a> {
    command: Vec<&'a str>,
    request_id: u32,
}

#[derive(Deserialize, Debug)]
struct IpcResponse {
    data: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Clone)]
pub struct MpvIpc {
    pub socket_path: PathBuf,
}

impl MpvIpc {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    pub fn cleanup(&self) -> Result<()> {
        match std::fs::remove_file(&self.socket_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("Failed to remove the mpv IPC socket"),
        }
    }

    #[cfg(unix)]
    pub async fn get_playback_position(&self) -> Result<Option<f64>> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixStream;

        if !self.socket_path.exists() {
            return Ok(None);
        }

        let stream = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            UnixStream::connect(&self.socket_path),
        )
        .await
        .context("Timed out connecting to mpv unix socket")?
        .context("Failed to connect to mpv unix socket")?;
        let mut stream = BufReader::new(stream);

        let cmd = IpcCommand {
            command: vec!["get_property", "time-pos"],
            request_id: 1,
        };

        let mut req = serde_json::to_vec(&cmd)?;
        req.push(b'\n');
        stream.get_mut().write_all(&req).await?;

        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            stream.read_line(&mut line),
        )
        .await
        .context("Timed out reading mpv IPC response")??;
        if read == 0 {
            return Ok(None);
        }

        if let Ok(response) = serde_json::from_str::<IpcResponse>(&line) {
            if response.error.as_deref() == Some("success") {
                return Ok(response.data.and_then(|value| value.as_f64()));
            }
        }

        Ok(None)
    }

    #[cfg(not(unix))]
    pub async fn get_playback_position(&self) -> Result<Option<f64>> {
        Ok(None)
    }
}
