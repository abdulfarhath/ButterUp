//! System health checks: the common broken states that make Debian
//! systems misbehave, each mapped to a guided repair where one exists.
//! All checks are read-only and run unprivileged.

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Repair {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: String,
    pub title: String,
    /// "ok" | "warn" | "bad" | "info" | "unknown"
    pub status: String,
    pub detail: String,
    pub repair: Option<Repair>,
}

impl Check {
    fn new(id: &str, title: &str, status: &str, detail: String, repair: Option<Repair>) -> Self {
        Check {
            id: id.into(),
            title: title.into(),
            status: status.into(),
            detail,
            repair,
        }
    }
}

fn repair(id: &str, label: &str) -> Option<Repair> {
    Some(Repair { id: id.into(), label: label.into() })
}

async fn cmd_output(program: &str, args: &[&str]) -> Result<(bool, String, String)> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .env("LC_ALL", "C.UTF-8")
        .output()
        .await?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// dpkg keeps journal files in /var/lib/dpkg/updates while it works;
/// leftover numeric entries mean an install/upgrade was interrupted.
fn check_dpkg_interrupted() -> Check {
    let (id, title) = ("dpkg-interrupted", "Interrupted package operation");
    match std::fs::read_dir("/var/lib/dpkg/updates") {
        Ok(entries) => {
            let pending = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .chars()
                        .all(|c| c.is_ascii_digit())
                })
                .count();
            if pending > 0 {
                Check::new(
                    id,
                    title,
                    "bad",
                    format!(
                        "dpkg was interrupted mid-operation ({pending} pending journal entries). \
                         Packages may be half-installed until it finishes."
                    ),
                    repair("configure-dpkg", "Finish pending configuration"),
                )
            } else {
                Check::new(id, title, "ok", "No interrupted dpkg operation.".into(), None)
            }
        }
        Err(e) => Check::new(id, title, "unknown", format!("Could not read dpkg state: {e}"), None),
    }
}

async fn check_dpkg_audit() -> Check {
    let (id, title) = ("dpkg-audit", "Package database consistency");
    match cmd_output("dpkg", &["--audit"]).await {
        Ok((_, stdout, _)) => {
            let report = stdout.trim();
            if report.is_empty() {
                Check::new(id, title, "ok", "dpkg reports no half-installed or misconfigured packages.".into(), None)
            } else {
                let mut detail: String = report.lines().take(12).collect::<Vec<_>>().join("\n");
                if report.lines().count() > 12 {
                    detail.push_str("\n…");
                }
                Check::new(
                    id,
                    title,
                    "bad",
                    detail,
                    repair("configure-dpkg", "Finish pending configuration"),
                )
            }
        }
        Err(e) => Check::new(id, title, "unknown", format!("dpkg --audit failed: {e}"), None),
    }
}

/// `apt-get -s -f install` simulates a fix-broken run; if it wants to
/// change anything, dependencies are currently broken.
async fn check_broken_deps() -> Check {
    let (id, title) = ("broken-deps", "Broken dependencies");
    match cmd_output("apt-get", &["-s", "-f", "install"]).await {
        Ok((true, stdout, _)) => {
            let clean = stdout
                .lines()
                .find(|l| l.contains("upgraded") && l.contains("newly installed"))
                .map(|l| l.trim_start().starts_with("0 upgraded, 0 newly installed, 0 to remove"))
                .unwrap_or(false);
            if clean {
                Check::new(id, title, "ok", "All package dependencies are satisfied.".into(), None)
            } else {
                Check::new(
                    id,
                    title,
                    "bad",
                    "apt wants to add or remove packages to repair dependencies.".into(),
                    repair("fix-broken", "Fix broken dependencies"),
                )
            }
        }
        Ok((false, _, stderr)) => {
            let detail = stderr.trim().lines().last().unwrap_or("apt reported an error").to_string();
            Check::new(id, title, "bad", detail, repair("fix-broken", "Fix broken dependencies"))
        }
        Err(e) => Check::new(id, title, "unknown", format!("apt-get check failed: {e}"), None),
    }
}

async fn check_held_packages() -> Check {
    let (id, title) = ("held-packages", "Held-back packages");
    match cmd_output("apt-mark", &["showhold"]).await {
        Ok((true, stdout, _)) => {
            let held: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
            if held.is_empty() {
                Check::new(id, title, "ok", "No packages are on hold.".into(), None)
            } else {
                Check::new(
                    id,
                    title,
                    "warn",
                    format!(
                        "{} package(s) held back from upgrades: {}. Holds are sometimes \
                         intentional; remove one with `sudo apt-mark unhold <pkg>`.",
                        held.len(),
                        held.join(", ")
                    ),
                    None,
                )
            }
        }
        Ok((false, _, stderr)) => Check::new(id, title, "unknown", stderr.trim().to_string(), None),
        Err(e) => Check::new(id, title, "unknown", format!("apt-mark failed: {e}"), None),
    }
}

/// A full /boot is the classic cause of failed kernel upgrades.
async fn check_boot_space() -> Check {
    let (id, title) = ("boot-space", "Space on /boot");
    match cmd_output("df", &["-P", "/boot"]).await {
        Ok((true, stdout, _)) => {
            let Some(line) = stdout.lines().nth(1) else {
                return Check::new(id, title, "unknown", "Unexpected df output.".into(), None);
            };
            let cols: Vec<&str> = line.split_whitespace().collect();
            let used_pct: u32 = cols
                .get(4)
                .and_then(|s| s.trim_end_matches('%').parse().ok())
                .unwrap_or(0);
            let avail_kb: u64 = cols.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let avail = human_kb(avail_kb);
            let (status, rep) = if used_pct >= 90 {
                ("bad", repair("autoremove", "Remove unused packages & old kernels"))
            } else if used_pct >= 75 {
                ("warn", repair("autoremove", "Remove unused packages & old kernels"))
            } else {
                ("ok", None)
            };
            Check::new(
                id,
                title,
                status,
                format!("{used_pct}% used, {avail} free. Old kernels are the usual culprit when this fills up."),
                rep,
            )
        }
        _ => Check::new(id, title, "unknown", "Could not stat /boot.".into(), None),
    }
}

/// Stale package lists mean "0 updates" can be a lie.
fn check_lists_freshness() -> Check {
    let (id, title) = ("lists-freshness", "Package list freshness");
    let newest = std::fs::read_dir("/var/lib/apt/lists")
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.metadata().ok()?.modified().ok())
                .max()
        })
        .flatten();
    let Some(newest) = newest else {
        return Check::new(id, title, "unknown", "Could not read /var/lib/apt/lists.".into(), None);
    };
    let days = std::time::SystemTime::now()
        .duration_since(newest)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0);
    if days >= 14 {
        Check::new(
            id,
            title,
            "warn",
            format!(
                "Package lists were last refreshed {days} days ago — the update list may be \
                 incomplete. Use “Check again” on the Updates tab to refresh."
            ),
            None,
        )
    } else {
        Check::new(
            id,
            title,
            "ok",
            format!(
                "Package lists refreshed {}.",
                if days == 0 { "today".into() } else { format!("{days} day(s) ago") }
            ),
            None,
        )
    }
}

/// Old kernels pile up in /boot until autoremove clears them.
async fn check_kernels() -> Check {
    let (id, title) = ("kernels", "Installed kernels");
    let running = cmd_output("uname", &["-r"])
        .await
        .map(|(_, out, _)| out.trim().to_string())
        .unwrap_or_default();
    match cmd_output("dpkg-query", &["-W", "-f=${Package} ${Status}\n", "linux-image-*"]).await {
        Ok((_, stdout, _)) => {
            let installed: Vec<&str> = stdout
                .lines()
                .filter(|l| l.ends_with("install ok installed"))
                .filter_map(|l| l.split_whitespace().next())
                .filter(|name| name.chars().any(|c| c.is_ascii_digit()))
                .collect();
            let detail = format!(
                "Running kernel {running} · {} kernel image(s) installed.",
                installed.len()
            );
            if installed.len() > 3 {
                Check::new(
                    id,
                    title,
                    "info",
                    format!("{detail} Older ones can be cleaned up safely."),
                    repair("autoremove", "Remove unused packages & old kernels"),
                )
            } else {
                Check::new(id, title, "ok", detail, None)
            }
        }
        Err(e) => Check::new(id, title, "unknown", format!("dpkg-query failed: {e}"), None),
    }
}

fn check_reboot_required() -> Check {
    let (id, title) = ("reboot-required", "Pending restart");
    if std::path::Path::new("/run/reboot-required").exists() {
        let pkgs = std::fs::read_to_string("/run/reboot-required.pkgs").unwrap_or_default();
        let pkgs: Vec<&str> = pkgs.lines().filter(|l| !l.trim().is_empty()).collect();
        let detail = if pkgs.is_empty() {
            "A previous update needs a restart to take effect.".to_string()
        } else {
            format!("Restart needed to finish updating: {}.", pkgs.join(", "))
        };
        Check::new(id, title, "warn", detail, None)
    } else {
        Check::new(id, title, "ok", "No restart pending.".into(), None)
    }
}

fn human_kb(kb: u64) -> String {
    human_bytes(kb * 1024)
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = b as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{b} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub async fn run_checks() -> Result<Vec<Check>> {
    let (audit, broken, held, boot, kernels) = tokio::join!(
        check_dpkg_audit(),
        check_broken_deps(),
        check_held_packages(),
        check_boot_space(),
        check_kernels(),
    );
    Ok(vec![
        check_dpkg_interrupted(),
        audit,
        broken,
        held,
        boot,
        kernels,
        check_lists_freshness(),
        check_reboot_required(),
    ])
}

/// Total download size for pending upgrades, e.g. "456 MB", from
/// apt's own simulation. Empty when nothing to fetch or on error.
pub async fn apt_download_size() -> String {
    let Ok((_, stdout, _)) = cmd_output("apt-get", &["-s", "dist-upgrade"]).await else {
        return String::new();
    };
    stdout
        .lines()
        .find(|l| l.starts_with("Need to get "))
        .and_then(|l| {
            let rest = l.strip_prefix("Need to get ")?;
            let size = rest.split(" of archives").next()?;
            // "1,234 kB/456 MB" means part is already cached; the total
            // is the number after the slash.
            Some(size.rsplit('/').next().unwrap_or(size).trim().to_string())
        })
        .filter(|s| !s.starts_with("0 "))
        .unwrap_or_default()
}

// ---- Cleanup ----

#[derive(Debug, Clone, Serialize)]
pub struct CleanupInfo {
    pub autoremovable: Vec<String>,
    pub autoremove_frees: String,
    pub cache_size: String,
    pub cache_files: usize,
    /// e.g. "1.5G", from `journalctl --disk-usage`; empty if unavailable.
    pub journal_size: String,
    pub old_snaps: Vec<crate::snapd::OldSnap>,
}

/// Size of systemd journals on disk, e.g. "1.5G".
async fn journal_usage() -> String {
    let Ok((ok, stdout, _)) = cmd_output("journalctl", &["--disk-usage"]).await else {
        return String::new();
    };
    if !ok {
        return String::new();
    }
    // "Archived and active journals take up 1.5G in the file system."
    stdout
        .split("take up ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|s| s.trim_end_matches('.').to_string())
        .unwrap_or_default()
}

/// Parse the package list out of `apt-get -s autoremove`: the indented
/// block following "The following packages will be REMOVED:".
fn parse_autoremove(stdout: &str) -> (Vec<String>, String) {
    let mut packages = Vec::new();
    let mut in_block = false;
    let mut frees = String::new();
    for line in stdout.lines() {
        if line.starts_with("The following packages will be REMOVED") {
            in_block = true;
            continue;
        }
        if in_block {
            if line.starts_with(' ') {
                packages.extend(line.split_whitespace().map(|s| s.trim_end_matches('*').to_string()));
            } else {
                in_block = false;
            }
        }
        // "After this operation, 123 MB disk space will be freed."
        if line.starts_with("After this operation") {
            if let (Some(start), Some(end)) = (line.find(',').map(|i| i + 1), line.find(" disk space")) {
                if start < end {
                    frees = line[start..end].trim().to_string();
                }
            }
        }
    }
    (packages, frees)
}

pub async fn cleanup_info() -> Result<CleanupInfo> {
    let autoremove = async {
        match cmd_output("apt-get", &["-s", "--purge", "autoremove"]).await {
            Ok((true, stdout, _)) => parse_autoremove(&stdout),
            _ => (Vec::new(), String::new()),
        }
    };
    let old_snaps = async { crate::snapd::old_revisions().await.unwrap_or_default() };
    let ((autoremovable, autoremove_frees), journal_size, old_snaps) =
        tokio::join!(autoremove, journal_usage(), old_snaps);

    let mut cache_bytes = 0u64;
    let mut cache_files = 0usize;
    if let Ok(entries) = std::fs::read_dir("/var/cache/apt/archives") {
        for e in entries.filter_map(|e| e.ok()) {
            if e.path().extension().map(|x| x == "deb").unwrap_or(false) {
                cache_files += 1;
                cache_bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    Ok(CleanupInfo {
        autoremovable,
        autoremove_frees,
        cache_size: human_bytes(cache_bytes),
        cache_files,
        journal_size,
        old_snaps,
    })
}
