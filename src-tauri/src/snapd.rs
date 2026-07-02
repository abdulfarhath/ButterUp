//! Snap updates via the snap CLI.
//!
//! `snap refresh --list` works unprivileged; the refresh itself runs
//! through pkexec so polkit handles authorization.

use anyhow::{bail, Result};
use serde::Serialize;

use crate::privileged;

#[derive(Debug, Clone, Serialize)]
pub struct SnapUpdate {
    pub name: String,
    pub version: String,
    pub size: String,
    pub publisher: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapStatus {
    pub available: bool,
    pub updates: Vec<SnapUpdate>,
}

fn snap_installed() -> bool {
    std::path::Path::new("/usr/bin/snap").exists() && std::path::Path::new("/run/snapd.socket").exists()
}

pub async fn list_updates() -> Result<SnapStatus> {
    if !snap_installed() {
        return Ok(SnapStatus { available: false, updates: Vec::new() });
    }

    let out = tokio::process::Command::new("snap")
        .args(["refresh", "--list"])
        .env("LC_ALL", "C.UTF-8")
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // "All snaps up to date." goes to stderr with exit code 0.
    if !out.status.success() {
        bail!("snap refresh --list failed: {}", stderr.trim());
    }

    let mut updates = Vec::new();
    for line in stdout.lines() {
        // Columns: Name Version Rev Size Publisher Notes
        if line.starts_with("Name") || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        updates.push(SnapUpdate {
            name: cols[0].to_string(),
            version: cols[1].to_string(),
            size: cols[3].to_string(),
            // snap marks verified publishers with a trailing ✓/✪ (or ** in
            // ASCII fallback); strip it, it's not part of the name.
            publisher: cols[4]
                .trim_end_matches(|c: char| matches!(c, '✓' | '✪' | '*'))
                .to_string(),
        });
    }

    Ok(SnapStatus { available: true, updates })
}

/// Refresh the named snaps (all pending if empty), streaming output lines.
pub async fn refresh(names: &[String], on_line: impl Fn(String)) -> Result<String> {
    let mut args = vec!["snap".to_string(), "refresh".to_string()];
    args.extend(names.iter().cloned());
    privileged::run_pkexec(&args, on_line).await
}

#[derive(Debug, Clone, Serialize)]
pub struct OldSnap {
    pub name: String,
    pub revision: String,
    pub version: String,
}

/// Superseded snap revisions still on disk. snapd keeps the previous
/// revisions of every snap ("disabled" in `snap list --all`); they are
/// safe to remove and often free gigabytes.
pub async fn old_revisions() -> Result<Vec<OldSnap>> {
    if !snap_installed() {
        return Ok(Vec::new());
    }

    let out = tokio::process::Command::new("snap")
        .args(["list", "--all"])
        .env("LC_ALL", "C.UTF-8")
        .output()
        .await?;
    if !out.status.success() {
        bail!("snap list --all failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut old = Vec::new();
    for line in stdout.lines() {
        // Columns: Name Version Rev Tracking Publisher Notes
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 6 && cols[5].contains("disabled") {
            old.push(OldSnap {
                name: cols[0].to_string(),
                version: cols[1].to_string(),
                revision: cols[2].to_string(),
            });
        }
    }
    Ok(old)
}

/// Remove all superseded revisions. One pkexec call per revision; polkit
/// caches the authorization so only the first one prompts.
pub async fn remove_old_revisions(on_line: impl Fn(String)) -> Result<String> {
    let old = old_revisions().await?;
    if old.is_empty() {
        return Ok("No old snap revisions to remove".into());
    }
    let total = old.len();
    for (i, snap) in old.iter().enumerate() {
        on_line(format!("Removing {} revision {} ({}/{})", snap.name, snap.revision, i + 1, total));
        let args = vec![
            "snap".to_string(),
            "remove".to_string(),
            snap.name.clone(),
            "--revision".to_string(),
            snap.revision.clone(),
        ];
        privileged::run_pkexec(&args, &on_line).await?;
    }
    Ok(format!("Removed {total} old snap revision(s)"))
}
