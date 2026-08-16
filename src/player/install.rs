use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug)]
pub enum PackageManager {
    Brew,
    Apt,
    Dnf,
    Pacman,
    Winget,
}

pub fn detect_package_manager() -> Option<PackageManager> {
    if which::which("brew").is_ok() {
        Some(PackageManager::Brew)
    } else if which::which("apt").is_ok() || which::which("apt-get").is_ok() {
        Some(PackageManager::Apt)
    } else if which::which("dnf").is_ok() {
        Some(PackageManager::Dnf)
    } else if which::which("pacman").is_ok() {
        Some(PackageManager::Pacman)
    } else if which::which("winget").is_ok() {
        Some(PackageManager::Winget)
    } else {
        None
    }
}

pub fn install_command(pm: &PackageManager) -> &'static str {
    match pm {
        PackageManager::Brew => "brew install mpv",
        PackageManager::Apt => "sudo apt update && sudo apt install -y mpv",
        PackageManager::Dnf => "sudo dnf install -y mpv",
        PackageManager::Pacman => "sudo pacman -S --noconfirm mpv",
        PackageManager::Winget => "winget install --id mpv.net --exact",
    }
}

pub fn run_package_install(pm: PackageManager) -> Result<()> {
    let success = match pm {
        PackageManager::Brew => run("brew", &["install", "mpv"])?,
        PackageManager::Apt => {
            run("sudo", &["apt", "update"])? && run("sudo", &["apt", "install", "-y", "mpv"])?
        }
        PackageManager::Dnf => run("sudo", &["dnf", "install", "-y", "mpv"])?,
        PackageManager::Pacman => run("sudo", &["pacman", "-S", "mpv"])?,
        PackageManager::Winget => run("winget", &["install", "--id", "mpv.net", "--exact"])?,
    };
    if !success {
        return Err(anyhow!("The package manager did not install mpv"));
    }
    Ok(())
}

fn run(program: &str, args: &[&str]) -> Result<bool> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("Failed to start {program}"))?;
    Ok(status.success())
}

pub fn verify_checksum(file_path: &Path, expected_hex: &str) -> Result<bool> {
    let file = File::open(file_path).context("Failed to open downloaded player")?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .context("Failed to read downloaded player")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let hash = hasher.finalize();
    let actual_hex = format!("{hash:x}");
    Ok(actual_hex.eq_ignore_ascii_case(expected_hex))
}

pub fn verify_and_finalize(partial: &Path, destination: &Path, expected_hex: &str) -> Result<()> {
    let verified = verify_checksum(partial, expected_hex);
    match verified {
        Ok(true) => {
            std::fs::rename(partial, destination)
                .context("Failed to move the verified player into place")?;
            Ok(())
        }
        Ok(false) => {
            remove_partial(partial)?;
            Err(anyhow!("Downloaded player checksum did not match"))
        }
        Err(error) => {
            let _ = remove_partial(partial);
            Err(error)
        }
    }
}

fn remove_partial(partial: &Path) -> Result<()> {
    match std::fs::remove_file(partial) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Failed to remove invalid player download"),
    }
}

pub fn confirm(reader: &mut impl BufRead) -> Result<bool> {
    let mut answer = String::new();
    reader
        .read_line(&mut answer)
        .context("Failed to read install confirmation")?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub fn prompt_install_guidance() -> Result<String> {
    if let Some(pm) = detect_package_manager() {
        let cmd = install_command(&pm);
        Ok(format!(
            "No media player found (mpv or VLC required).\nYou can install mpv by running:\n  {}",
            cmd
        ))
    } else {
        Ok("No supported package manager found.\nPlease install mpv from https://mpv.io/installation/ or VLC from https://www.videolan.org/vlc/".to_string())
    }
}
