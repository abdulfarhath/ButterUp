//! Flatpak updates via the flatpak CLI.
//!
//! Listing and updating run unprivileged; for system-wide installations
//! flatpak escalates through polkit on its own.

use anyhow::{bail, Result};
use serde::Serialize;

use crate::privileged;

#[derive(Debug, Clone, Serialize)]
pub struct FlatpakUpdate {
    pub application: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlatpakStatus {
    pub available: bool,
    pub updates: Vec<FlatpakUpdate>,
}

fn flatpak_installed() -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join("flatpak").is_file()))
        .unwrap_or(false)
}

pub async fn list_updates() -> Result<FlatpakStatus> {
    if !flatpak_installed() {
        return Ok(FlatpakStatus { available: false, updates: Vec::new() });
    }

    let out = tokio::process::Command::new("flatpak")
        .args(["remote-ls", "--updates", "--columns=application,name,version"])
        .env("LC_ALL", "C.UTF-8")
        .output()
        .await?;

    if !out.status.success() {
        bail!(
            "flatpak remote-ls failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let updates = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut cols = line.split('\t');
            FlatpakUpdate {
                application: cols.next().unwrap_or_default().trim().to_string(),
                name: cols.next().unwrap_or_default().trim().to_string(),
                version: cols.next().unwrap_or_default().trim().to_string(),
            }
        })
        .filter(|u| !u.application.is_empty())
        .collect();

    Ok(FlatpakStatus { available: true, updates })
}

/// Update the given application IDs (all pending if empty), streaming output.
pub async fn update(applications: &[String], on_line: impl Fn(String)) -> Result<String> {
    let mut cmd = tokio::process::Command::new("flatpak");
    cmd.args(["update", "-y", "--noninteractive"])
        .args(applications)
        .env("LC_ALL", "C.UTF-8");

    let outcome = privileged::stream_command(cmd, on_line).await?;
    if outcome.code != Some(0) {
        bail!(
            "flatpak update failed: {}",
            if outcome.err_tail.is_empty() {
                outcome.last_line
            } else {
                outcome.err_tail.join(" / ")
            }
        );
    }
    Ok("Flatpak apps updated".to_string())
}
