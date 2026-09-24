use anyhow::{anyhow, bail, Context, Result};
use futures_util::{Stream, StreamExt};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/BabalolaBrainiac/flix/releases/latest";
pub const CHECKSUMS_NAME: &str = "SHA256SUMS.txt";
pub const MACOS_PACKAGE: &str = "Flix-macOS-universal.dmg";
pub const WINDOWS_PACKAGE: &str = "Flix-Windows-x64-Setup.exe";
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const METADATA_TIMEOUT: Duration = Duration::from_secs(15);
const PACKAGE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInstallResult {
    pub version: String,
    pub package_path: String,
    pub requires_manual_finish: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}

pub trait UpdateLauncher: Send + Sync {
    fn launch(&self, package: &Path, pid: u32) -> Result<bool>;
}

#[derive(Clone, Default)]
pub struct RealLauncher;

impl UpdateLauncher for RealLauncher {
    fn launch(&self, package: &Path, pid: u32) -> Result<bool> {
        launch_platform_installer(package, pid)
    }
}

struct TempFileGuard {
    path: PathBuf,
    active: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub trait UpdateAdapter: Send + Sync {
    fn check<'a>(&'a self) -> futures_util::future::BoxFuture<'a, Result<UpdateStatus>>;
    fn download_and_launch<'a>(
        &'a self,
    ) -> futures_util::future::BoxFuture<'a, Result<UpdateInstallResult>>;
}

#[derive(Clone)]
pub struct Updater {
    client: reqwest::Client,
    data_dir: PathBuf,
    install_lock: Arc<Mutex<()>>,
    launcher: Arc<dyn UpdateLauncher>,
    latest_url: String,
    adapter: Option<Arc<dyn UpdateAdapter>>,
}

impl Updater {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Flix/", env!("CARGO_PKG_VERSION")))
            .https_only(true)
            .redirect(reqwest::redirect::Policy::limited(5))
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .context("Failed to build the update client")?;
        Ok(Self {
            client,
            data_dir: data_dir.to_path_buf(),
            install_lock: Arc::new(Mutex::new(())),
            launcher: Arc::new(RealLauncher),
            latest_url: LATEST_RELEASE_URL.to_string(),
            adapter: None,
        })
    }

    pub fn with_launcher(mut self, launcher: Arc<dyn UpdateLauncher>) -> Self {
        self.launcher = launcher;
        self
    }

    pub fn with_adapter(mut self, adapter: Arc<dyn UpdateAdapter>) -> Self {
        self.adapter = Some(adapter);
        self
    }

    pub fn with_latest_url(mut self, url: String) -> Self {
        self.latest_url = url;
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub fn install_lock(&self) -> Arc<Mutex<()>> {
        self.install_lock.clone()
    }

    pub async fn check(&self) -> Result<UpdateStatus> {
        if let Some(adapter) = &self.adapter {
            return adapter.check().await;
        }
        let release = self.latest_release().await?;
        let current = env!("CARGO_PKG_VERSION");
        let version = parse_release_version(&release.tag_name)?;
        Ok(UpdateStatus {
            current_version: current.to_string(),
            latest_version: version.to_string(),
            available: update_is_available(current, &release.tag_name)?,
            release_url: release.html_url,
        })
    }

    pub async fn download_and_launch(&self) -> Result<UpdateInstallResult> {
        let _guard = self
            .install_lock
            .try_lock()
            .map_err(|_| anyhow!("An update installation is already in progress"))?;

        if let Some(adapter) = &self.adapter {
            return adapter.download_and_launch().await;
        }

        let release = self.latest_release().await?;
        if !update_is_available(env!("CARGO_PKG_VERSION"), &release.tag_name)? {
            bail!("Flix is already up to date");
        }

        let package_name = platform_package_name(std::env::consts::OS)?;
        let checksums = find_asset(&release, CHECKSUMS_NAME)?;
        let package = find_asset(&release, package_name)?;
        let checksum_document = self.download_text(checksums).await?;
        let expected_checksum = parse_checksum(&checksum_document, package_name)?;
        let version = parse_release_version(&release.tag_name)?;
        let version_str = version.to_string();
        let directory = self.data_dir.join("updates").join(&version_str);
        tokio::fs::create_dir_all(&directory)
            .await
            .context("Failed to create the update directory")?;
        let package_path = directory.join(package_name);
        self.download_verified(package, &package_path, &expected_checksum)
            .await?;
        let pid = std::process::id();
        let requires_manual_finish = self.launcher.launch(&package_path, pid)?;

        Ok(UpdateInstallResult {
            version: version_str,
            package_path: package_path.to_string_lossy().into_owned(),
            requires_manual_finish,
        })
    }

    async fn latest_release(&self) -> Result<Release> {
        let response = self
            .client
            .get(&self.latest_url)
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .context("Failed to check for updates")?
            .error_for_status()
            .context("The update service returned an error")?;
        reject_large_response(&response, MAX_METADATA_BYTES)?;
        let release: Release = response
            .json()
            .await
            .context("The update service returned invalid metadata")?;
        if release.draft || release.prerelease {
            bail!("The latest release is not a stable release");
        }
        update_is_available(env!("CARGO_PKG_VERSION"), &release.tag_name)?;
        Ok(release)
    }

    async fn download_text(&self, asset: &ReleaseAsset) -> Result<String> {
        validate_download_url(&asset.browser_download_url)?;
        let response = self
            .client
            .get(&asset.browser_download_url)
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .context("Failed to download release checksums")?
            .error_for_status()
            .context("The checksum download failed")?;
        reject_large_response(&response, MAX_METADATA_BYTES)?;
        let bytes = response.bytes().await?;
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            bail!("The checksum document is too large");
        }
        String::from_utf8(bytes.to_vec()).context("The checksum document is not UTF-8")
    }

    async fn download_verified(
        &self,
        asset: &ReleaseAsset,
        destination: &Path,
        expected_checksum: &str,
    ) -> Result<()> {
        validate_download_url(&asset.browser_download_url)?;
        let response = self
            .client
            .get(&asset.browser_download_url)
            .timeout(PACKAGE_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to download the update package")?
            .error_for_status()
            .context("The update package download failed")?;
        reject_large_response(&response, MAX_PACKAGE_BYTES)?;

        let stream = response.bytes_stream();
        write_verified_stream(stream, destination, expected_checksum).await
    }
}

pub async fn write_verified_stream<S, B, E>(
    stream: S,
    destination: &Path,
    expected_checksum: &str,
) -> Result<()>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    write_verified_stream_with_limit(stream, destination, expected_checksum, MAX_PACKAGE_BYTES)
        .await
}

pub async fn write_verified_stream_with_limit<S, B, E>(
    mut stream: S,
    destination: &Path,
    expected_checksum: &str,
    max_bytes: u64,
) -> Result<()>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    let temporary = destination.with_extension("download");
    let _ = tokio::fs::remove_file(&temporary).await;
    let mut guard = TempFileGuard::new(temporary.clone());

    let mut file = tokio::fs::File::create(&temporary)
        .await
        .context("Failed to create the update package")?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("The update download was interrupted")?;
        let slice = chunk.as_ref();
        size = size.saturating_add(slice.len() as u64);
        if size > max_bytes {
            bail!("The update package is too large");
        }
        hash.update(slice);
        file.write_all(slice).await?;
    }
    file.flush().await?;
    drop(file);

    let actual = format!("{:x}", hash.finalize());
    if actual != expected_checksum {
        bail!("The update package checksum does not match");
    }

    if destination.exists() {
        tokio::fs::remove_file(destination)
            .await
            .context("Failed to replace existing destination package")?;
    }

    tokio::fs::rename(&temporary, destination)
        .await
        .context("Failed to finish the update download")?;
    guard.disarm();
    Ok(())
}

fn reject_large_response(response: &reqwest::Response, limit: u64) -> Result<()> {
    if response.content_length().is_some_and(|size| size > limit) {
        bail!("The update response is too large");
    }
    Ok(())
}

pub fn find_asset<'a>(release: &'a Release, name: &str) -> Result<&'a ReleaseAsset> {
    let matches: Vec<_> = release
        .assets
        .iter()
        .filter(|asset| asset.name == name)
        .collect();
    match matches.as_slice() {
        [asset] => Ok(asset),
        [] => Err(anyhow!("The release does not contain {name}")),
        _ => Err(anyhow!("The release contains duplicate {name} assets")),
    }
}

pub fn validate_download_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("The release contains an invalid download URL")?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        bail!("The release contains an untrusted download URL");
    }
    Ok(())
}

pub fn platform_package_name(os: &str) -> Result<&'static str> {
    match os {
        "macos" => Ok(MACOS_PACKAGE),
        "windows" => Ok(WINDOWS_PACKAGE),
        _ => bail!("Automatic updates are not available on {os}"),
    }
}

pub fn parse_checksum(document: &str, package_name: &str) -> Result<String> {
    let matches: Vec<_> = document
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let checksum = fields.next()?;
            let path = fields.next()?;
            (Path::new(path).file_name()?.to_str()? == package_name).then_some(checksum)
        })
        .collect();
    let [checksum] = matches.as_slice() else {
        bail!("The release checksum for {package_name} is missing or ambiguous");
    };
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("The release checksum for {package_name} is invalid");
    }
    Ok(checksum.to_ascii_lowercase())
}

pub fn parse_release_version(tag: &str) -> Result<Version> {
    let clean = tag.strip_prefix('v').unwrap_or(tag);
    if clean.starts_with('v') {
        bail!("Malformed release tag: {tag}");
    }
    Version::parse(clean).context("The release version is invalid")
}

pub fn update_is_available(current: &str, latest_tag: &str) -> Result<bool> {
    let current_version = Version::parse(current).context("The current Flix version is invalid")?;
    let latest_version = parse_release_version(latest_tag)?;
    Ok(latest_version > current_version)
}

pub fn build_windows_helper_command(pid: u32, package: &Path) -> (&'static str, Vec<String>) {
    let escaped_path = package.to_string_lossy().replace('\'', "''");
    let script = format!(
        "try {{ Wait-Process -Id {pid} -ErrorAction Stop }} catch {{}}; Start-Process -FilePath '{escaped_path}' -ArgumentList '/S'"
    );
    (
        "powershell",
        vec![
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-WindowStyle".to_string(),
            "Hidden".to_string(),
            "-Command".to_string(),
            script,
        ],
    )
}

#[cfg(target_os = "macos")]
fn launch_platform_installer(package: &Path, _pid: u32) -> Result<bool> {
    std::process::Command::new("/usr/bin/open")
        .arg(package)
        .spawn()
        .context("Failed to open the macOS update package")?;
    Ok(true)
}

#[cfg(target_os = "windows")]
fn launch_platform_installer(package: &Path, pid: u32) -> Result<bool> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let (program, args) = build_windows_helper_command(pid, package);
    std::process::Command::new(program)
        .args(args)
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .context("Failed to start the Windows update installer")?;
    Ok(false)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn launch_platform_installer(_package: &Path, _pid: u32) -> Result<bool> {
    bail!("Automatic updates are not available on this operating system")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use tempfile::TempDir;

    #[test]
    fn selects_the_native_package() {
        assert_eq!(platform_package_name("macos").unwrap(), MACOS_PACKAGE);
        assert_eq!(platform_package_name("windows").unwrap(), WINDOWS_PACKAGE);
        assert!(platform_package_name("linux").is_err());
    }

    #[test]
    fn reads_a_checksum_from_nested_release_paths() {
        let hash = "a".repeat(64);
        let document = format!(
            "{hash}  release-artifacts/Flix-macOS-universal.dmg/Flix-macOS-universal.dmg\n{}  release-artifacts/Flix-Windows-x64-Setup.exe/Flix-Windows-x64-Setup.exe\n",
            "c".repeat(64),
        );
        assert_eq!(parse_checksum(&document, MACOS_PACKAGE).unwrap(), hash);
    }

    #[test]
    fn rejects_missing_and_duplicate_checksums() {
        assert!(parse_checksum("not-a-hash  Flix-macOS-universal.dmg", MACOS_PACKAGE).is_err());
        assert!(parse_checksum("", MACOS_PACKAGE).is_err());
        let duplicate = format!(
            "{}  {MACOS_PACKAGE}\n{}  {MACOS_PACKAGE}\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        assert!(parse_checksum(&duplicate, MACOS_PACKAGE).is_err());
    }

    #[test]
    fn compares_semantic_versions() {
        assert!(update_is_available("0.3.0", "v0.4.0").unwrap());
        assert!(!update_is_available("0.4.0", "v0.4.0").unwrap());
        assert!(!update_is_available("0.4.1", "v0.4.0").unwrap());
        assert!(update_is_available("bad", "v0.4.0").is_err());
    }

    #[test]
    fn rejects_malformed_release_tags() {
        assert!(parse_release_version("v0.4.0").is_ok());
        assert!(parse_release_version("0.4.0").is_ok());
        assert!(parse_release_version("vv0.4.0").is_err());
        assert!(parse_release_version("vvv0.4.0").is_err());
        assert!(parse_release_version("v").is_err());
        assert!(parse_release_version("v0.4").is_err());
        assert!(parse_release_version("invalid").is_err());
    }

    #[test]
    fn rejects_duplicate_or_missing_package_assets() {
        let release = Release {
            tag_name: "v0.4.0".to_string(),
            html_url: "https://github.com/BabalolaBrainiac/flix/releases/tag/v0.4.0".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![
                ReleaseAsset {
                    name: MACOS_PACKAGE.to_string(),
                    browser_download_url: "https://github.com/download/macos1".to_string(),
                },
                ReleaseAsset {
                    name: MACOS_PACKAGE.to_string(),
                    browser_download_url: "https://github.com/download/macos2".to_string(),
                },
            ],
        };
        assert!(find_asset(&release, MACOS_PACKAGE).is_err());
        assert!(find_asset(&release, WINDOWS_PACKAGE).is_err());
    }

    #[test]
    fn rejects_non_github_download_urls() {
        assert!(validate_download_url("http://github.com/file").is_err());
        assert!(validate_download_url("https://example.com/file").is_err());
        assert!(validate_download_url("https://github.com/BabalolaBrainiac/flix/file").is_ok());
    }

    #[tokio::test]
    async fn rejects_checksum_mismatches_and_cleans_temporary_file() {
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("pkg.dmg");
        let temporary = destination.with_extension("download");
        let content = b"package-payload-bytes".to_vec();
        let stream = stream::iter(vec![Ok::<_, std::io::Error>(content)]);
        let wrong_hash = "0".repeat(64);

        let result = write_verified_stream(stream, &destination, &wrong_hash).await;
        assert!(result.is_err());
        assert!(
            !temporary.exists(),
            "temporary download file must be removed"
        );
        assert!(
            !destination.exists(),
            "destination file must not be created"
        );
    }

    #[tokio::test]
    async fn removes_partial_file_after_stream_failure() {
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("pkg.dmg");
        let temporary = destination.with_extension("download");
        let stream = stream::iter(vec![
            Ok::<_, std::io::Error>(b"part-1".to_vec()),
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "interrupted",
            )),
        ]);
        let hash = "a".repeat(64);

        let result = write_verified_stream(stream, &destination, &hash).await;
        assert!(result.is_err());
        assert!(
            !temporary.exists(),
            "temporary download file must be removed on error"
        );
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn rejects_an_oversized_streamed_package() {
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("pkg.dmg");
        let temporary = destination.with_extension("download");
        let stream = stream::iter(vec![
            Ok::<_, std::io::Error>(vec![0u8; 100]),
            Ok(vec![0u8; 100]),
        ]);
        let hash = "a".repeat(64);

        let result = write_verified_stream_with_limit(stream, &destination, &hash, 150).await;
        assert!(result.is_err());
        assert!(
            !temporary.exists(),
            "temporary file must be removed when oversized"
        );
    }

    #[tokio::test]
    async fn replaces_existing_destination_package_safely() {
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("pkg.dmg");
        std::fs::write(&destination, b"old-package-content").unwrap();

        let new_content = b"new-package-verified-content".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&new_content);
        let expected_hash = format!("{:x}", hasher.finalize());

        let stream = stream::iter(vec![Ok::<_, std::io::Error>(new_content.clone())]);
        write_verified_stream(stream, &destination, &expected_hash)
            .await
            .unwrap();

        let updated = std::fs::read(&destination).unwrap();
        assert_eq!(updated, new_content);
        assert!(!destination.with_extension("download").exists());
    }

    #[tokio::test]
    async fn rejects_concurrent_installation_request() {
        let dir = TempDir::new().unwrap();
        let updater = Updater::new(dir.path()).unwrap();
        let lock = updater.install_lock();

        let _guard = lock.try_lock().expect("must acquire lock initially");
        let second_attempt = lock.try_lock();
        assert!(second_attempt.is_err(), "concurrent acquisition must fail");
    }

    #[test]
    fn builds_windows_helper_command_with_pid_and_silent_flag() {
        let package =
            Path::new(r"C:\Users\test\AppData\Local\flix\updates\0.4.0\Flix-Windows-x64-Setup.exe");
        let (program, args) = build_windows_helper_command(4321, package);
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
        assert!(script.contains("Wait-Process -Id 4321"));
        assert!(script.contains(
            r"C:\Users\test\AppData\Local\flix\updates\0.4.0\Flix-Windows-x64-Setup.exe"
        ));
        assert!(script.contains("/S"));
    }

    #[test]
    fn builds_windows_helper_command_escapes_special_path_characters() {
        let package = Path::new(r"C:\Users\test$user`name's\updates\setup.exe");
        let (_, args) = build_windows_helper_command(1234, package);
        let cmd_idx = args
            .iter()
            .position(|a| a == "-Command")
            .expect("has -Command");
        let script = &args[cmd_idx + 1];
        assert!(script.contains(r"Start-Process -FilePath 'C:\Users\test$user`name''s\updates\setup.exe' -ArgumentList '/S'"));
    }
}
