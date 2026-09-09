use anyhow::{anyhow, Result};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn first_subtitle(
    first: impl Future<Output = Result<Option<PathBuf>>>,
    second: impl Future<Output = Result<Option<PathBuf>>>,
) -> Result<Option<PathBuf>> {
    tokio::pin!(first, second);
    let (result, remaining) = tokio::select! {
        result = &mut first => (result, second.as_mut()),
        result = &mut second => return match result {
            Ok(Some(path)) => Ok(Some(path)),
            _ => first.await,
        },
    };
    match result {
        Ok(Some(path)) => Ok(Some(path)),
        _ => remaining.await,
    }
}

pub async fn prepare_with_subtitles<T>(
    warmup: impl Future<Output = Result<T>>,
    subtitles: impl Future<Output = Vec<PathBuf>>,
    cancel: &CancellationToken,
    subtitle_grace: Duration,
) -> Result<(T, Vec<PathBuf>)> {
    tokio::pin!(warmup, subtitles);
    let ready = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(anyhow!("Operation cancelled")),
        ready = &mut warmup => ready?,
        paths = &mut subtitles => return tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(anyhow!("Operation cancelled")),
            ready = warmup => Ok((ready?, paths)),
        },
    };
    let paths = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(anyhow!("Operation cancelled")),
        paths = tokio::time::timeout(subtitle_grace, subtitles) => paths.unwrap_or_default(),
    };
    Ok((ready, paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready};

    #[tokio::test]
    async fn uses_a_ready_subtitle_without_waiting_for_another_provider() {
        let path = PathBuf::from("english.srt");
        let result = first_subtitle(pending(), ready(Ok(Some(path.clone())))).await;
        assert_eq!(result.unwrap(), Some(path));
    }

    #[tokio::test]
    async fn continues_after_a_subtitle_provider_fails() {
        let path = PathBuf::from("english.srt");
        let result = first_subtitle(
            ready(Err(anyhow!("unavailable"))),
            ready(Ok(Some(path.clone()))),
        );
        assert_eq!(result.await.unwrap(), Some(path));
    }

    #[tokio::test]
    async fn stops_subtitle_work_when_buffering_fails() {
        let result = prepare_with_subtitles::<()>(
            ready(Err(anyhow!("buffering failed"))),
            pending(),
            &CancellationToken::new(),
            Duration::from_secs(25),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("buffering failed"));
    }

    #[tokio::test]
    async fn limits_the_subtitle_wait_after_buffering() {
        let result = prepare_with_subtitles(
            ready(Ok(())),
            pending(),
            &CancellationToken::new(),
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert!(result.1.is_empty());
    }

    #[tokio::test]
    async fn cancels_pending_preparation() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result =
            prepare_with_subtitles::<()>(pending(), pending(), &cancel, Duration::from_secs(25))
                .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Operation cancelled"));
    }
}
