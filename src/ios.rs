use crate::config::Config;
use crate::desktop::{DesktopServer, DesktopService};
use crate::player::embedded::{
    self, clear_host, register_host, EmbeddedLaunchRequest, EmbeddedPlayerHost,
};
use anyhow::{anyhow, Result};
use std::ffi::{c_char, CString};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Runtime;
use tokio::sync::broadcast;

pub type LaunchCallback = extern "C" fn(*const c_char) -> i32;
pub type StopCallback = extern "C" fn() -> i32;

struct FfiPlayerHost {
    on_launch: LaunchCallback,
    on_stop: StopCallback,
}

impl EmbeddedPlayerHost for FfiPlayerHost {
    fn launch(&self, request: EmbeddedLaunchRequest) -> Result<()> {
        let payload = serde_json::to_string(&request)?;
        let c_payload = CString::new(payload)?;
        let status = (self.on_launch)(c_payload.as_ptr());
        if status == 0 {
            Ok(())
        } else {
            Err(anyhow!(
                "The embedded iOS player rejected the launch request (code {status})"
            ))
        }
    }

    fn stop(&self) -> Result<()> {
        let status = (self.on_stop)();
        if status == 0 {
            Ok(())
        } else {
            Err(anyhow!(
                "The embedded iOS player failed to stop (code {status})"
            ))
        }
    }
}

struct RunningServer {
    launch_url: String,
    shutdown_tx: broadcast::Sender<()>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static SERVER: OnceLock<Mutex<Option<RunningServer>>> = OnceLock::new();
static LOGGING_INIT: OnceLock<()> = OnceLock::new();

fn runtime() -> Result<&'static Runtime> {
    if let Some(existing) = RUNTIME.get() {
        return Ok(existing);
    }
    let created = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _ = RUNTIME.set(created);
    RUNTIME
        .get()
        .ok_or_else(|| anyhow!("Could not initialize the iOS async runtime"))
}

fn server_slot() -> &'static Mutex<Option<RunningServer>> {
    SERVER.get_or_init(|| Mutex::new(None))
}

fn init_logging_once() {
    LOGGING_INIT.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new("flix=info,librqbit=error,warn")
        });
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    });
}

fn start_internal(port: u16) -> Result<String> {
    init_logging_once();
    if let Some(existing) = server_slot().lock().unwrap().as_ref() {
        return Ok(existing.launch_url.clone());
    }

    let rt = runtime()?;
    rt.block_on(async move {
        let _ = crate::process_limits::raise_open_file_limit();
        crate::config::schedule_orphaned_playback_cache_sweep();

        let config = Config::load()?;
        let cache_data = config.data_dir.clone();
        tokio::task::spawn_blocking(move || crate::storage::trim_media_caches(&cache_data));

        let service = DesktopService::new(config)?;
        let server = if port > 0 {
            DesktopServer::bind_on(service, port).await?
        } else {
            DesktopServer::bind(service).await?
        };
        let launch_url = server.url();
        let shutdown_tx = server.state.shutdown_tx.clone();
        tokio::spawn(async move {
            let _ = server.run().await;
        });

        *server_slot().lock().unwrap() = Some(RunningServer {
            launch_url: launch_url.clone(),
            shutdown_tx,
        });
        Ok(launch_url)
    })
}

/// Registers the C callbacks that launch and stop the native iOS player view.
#[no_mangle]
pub extern "C" fn flix_ios_register_player_callbacks(
    on_launch: LaunchCallback,
    on_stop: StopCallback,
) {
    register_host(Arc::new(FfiPlayerHost { on_launch, on_stop }));
}

/// Clears the registered native iOS player callbacks.
#[no_mangle]
pub extern "C" fn flix_ios_clear_player_callbacks() {
    clear_host();
}

/// Starts the embedded Flix server on `127.0.0.1` and returns the launch URL.
/// The caller must release the returned string with `flix_ios_free_string`.
#[no_mangle]
pub extern "C" fn flix_ios_start(port: u16) -> *mut c_char {
    match start_internal(port) {
        Ok(url) => CString::new(url)
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        Err(error) => {
            tracing::error!("Failed to start Flix iOS server: {error:#}");
            std::ptr::null_mut()
        }
    }
}

/// Stops the embedded Flix server if it is running.
#[no_mangle]
pub extern "C" fn flix_ios_stop() {
    if let Some(running) = server_slot().lock().unwrap().take() {
        let _ = running.shutdown_tx.send(());
    }
}

/// Reports that the native iOS player finished or closed.
#[no_mangle]
pub extern "C" fn flix_ios_report_player_exit(success: bool, exit_code: i32) {
    embedded::report_exit(success, Some(exit_code));
}

/// Releases a C string allocated by `flix_ios_start`.
///
/// # Safety
///
/// `ptr` must be a pointer returned by `flix_ios_start` or null.
#[no_mangle]
pub unsafe extern "C" fn flix_ios_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{self, PlaybackOptions};
    use std::ffi::CStr;

    static LAST_LAUNCH_JSON: OnceLock<Mutex<Option<String>>> = OnceLock::new();

    extern "C" fn record_launch(payload: *const c_char) -> i32 {
        let text = unsafe { CStr::from_ptr(payload) }
            .to_string_lossy()
            .into_owned();
        *LAST_LAUNCH_JSON.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(text);
        0
    }

    extern "C" fn record_stop() -> i32 {
        0
    }

    #[tokio::test]
    async fn registers_callbacks_launches_and_reports_exit() {
        flix_ios_register_player_callbacks(record_launch, record_stop);
        let detected = player::detect();
        let embedded_player = detected
            .iter()
            .find(|candidate| embedded::is_embedded_player(candidate))
            .expect("embedded player detected");

        let opts = PlaybackOptions::new("Sample Movie".to_string(), 1024);
        let mut managed =
            player::launch(embedded_player, "http://127.0.0.1:8080/s/0/0?t=1", &opts).unwrap();
        managed.wait_for_startup().await.unwrap();
        assert!(!managed.has_exited().unwrap());

        let json = LAST_LAUNCH_JSON
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .clone()
            .expect("launch payload");
        assert!(json.contains("Sample Movie"));

        flix_ios_report_player_exit(true, 0);
        let exit = managed.poll_exit().unwrap().expect("exit reported");
        assert!(exit.success);
        assert_eq!(exit.exit_code, Some(0));

        managed.stop().unwrap();
        flix_ios_clear_player_callbacks();
    }
}
