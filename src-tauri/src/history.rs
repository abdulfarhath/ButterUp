//! Update history from apt's own transaction log.
//!
//! Every apt/dpkg transaction — including ones driven through
//! PackageKit — is appended to /var/log/apt/history.log as a blank-line
//! separated block of "Key: value" lines. logrotate keeps the previous
//! month in history.log.1.gz; we include it for more depth.

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub date: String,
    pub command: String,
    pub requested_by: String,
    /// Rendered as "name old → new".
    pub upgraded: Vec<String>,
    /// Rendered as "name version".
    pub installed: Vec<String>,
    pub removed: Vec<String>,
}

/// Split a package-list value like
/// `foo:amd64 (1.0, 1.1), bar:amd64 (2.0, automatic)` into entries.
fn parse_packages(value: &str, upgrade: bool) -> Vec<String> {
    value
        .split("), ")
        .map(|chunk| chunk.trim_end_matches(')'))
        .filter_map(|chunk| {
            let (name_arch, versions) = chunk.split_once(" (")?;
            let name = name_arch.split(':').next().unwrap_or(name_arch);
            let versions = versions.replace(", automatic", "");
            if upgrade {
                let (old, new) = versions.split_once(", ").unwrap_or((versions.as_str(), ""));
                if new.is_empty() {
                    Some(format!("{name} {old}"))
                } else {
                    Some(format!("{name} {old} → {new}"))
                }
            } else {
                Some(format!("{name} {versions}"))
            }
        })
        .collect()
}

fn parse_log(content: &str, entries: &mut Vec<HistoryEntry>) {
    for block in content.split("\n\n") {
        let mut entry = HistoryEntry {
            date: String::new(),
            command: String::new(),
            requested_by: String::new(),
            upgraded: Vec::new(),
            installed: Vec::new(),
            removed: Vec::new(),
        };
        for line in block.lines() {
            let Some((key, value)) = line.split_once(": ") else { continue };
            match key {
                "Start-Date" => entry.date = value.trim().to_string(),
                "Commandline" => entry.command = value.trim().to_string(),
                "Requested-By" => entry.requested_by = value.trim().to_string(),
                "Upgrade" => entry.upgraded = parse_packages(value, true),
                "Install" => entry.installed = parse_packages(value, false),
                "Reinstall" => entry.installed.extend(parse_packages(value, false)),
                "Remove" | "Purge" => entry.removed.extend(parse_packages(value, false)),
                _ => {}
            }
        }
        let has_changes =
            !entry.upgraded.is_empty() || !entry.installed.is_empty() || !entry.removed.is_empty();
        if !entry.date.is_empty() && has_changes {
            entries.push(entry);
        }
    }
}

/// Latest-first history, capped so the UI stays snappy.
pub async fn get_history(limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut entries = Vec::new();

    // Rotated log first so the concatenation stays chronological.
    let rotated = tokio::process::Command::new("gzip")
        .args(["-dc", "/var/log/apt/history.log.1.gz"])
        .output()
        .await;
    if let Ok(out) = rotated {
        if out.status.success() {
            parse_log(&String::from_utf8_lossy(&out.stdout), &mut entries);
        }
    }

    if let Ok(current) = std::fs::read_to_string("/var/log/apt/history.log") {
        parse_log(&current, &mut entries);
    }

    entries.reverse();
    entries.truncate(limit);
    Ok(entries)
}
