//! PackageKit D-Bus client: query and install apt updates.
//!
//! Flow: CreateTransaction on the main PackageKit object returns a
//! per-transaction object path; calling GetUpdates on it streams one
//! `Package` signal per pending update, then a `Finished` signal.
//! UpdatePackages works the same way and additionally reports progress
//! through the `Percentage` property and `Package` signals. Privilege
//! escalation is handled by PackageKit itself via polkit.

use anyhow::{bail, Result};
use futures_util::StreamExt;
use serde::Serialize;
use zbus::zvariant::OwnedObjectPath;

/// PK_FILTER_ENUM_NONE — PackageKit filters are a bitfield.
const FILTER_NONE: u64 = 1 << 1;
/// PK_TRANSACTION_FLAG_ENUM_ONLY_TRUSTED.
const FLAG_ONLY_TRUSTED: u64 = 1 << 1;
/// PK_EXIT_ENUM_SUCCESS.
const EXIT_SUCCESS: u32 = 1;
/// PK_RESTART_ENUM_SYSTEM / PK_RESTART_ENUM_SECURITY_SYSTEM.
const RESTART_SYSTEM: u32 = 4;
const RESTART_SECURITY_SYSTEM: u32 = 6;
/// The `Percentage` property reports 101 when unknown.
const PERCENTAGE_UNKNOWN: u32 = 101;

#[derive(Debug, Clone, Serialize)]
pub struct Update {
    pub package_id: String,
    pub name: String,
    pub version: String,
    pub summary: String,
    pub severity: String,
    /// Raw PK_INFO_ENUM value, kept for diagnostics.
    pub info: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub needs_restart: bool,
}

/// PK_INFO_ENUM_* values reported per package by GetUpdates.
/// Since PackageKit 1.3 the Package signal packs an update-severity enum
/// into the high 16 bits of `info`; the low 16 bits keep the base
/// info/action enum (INSTALL/REMOVE/OBSOLETE/DOWNGRADE are 27–30).
fn severity_from_pk_info(info: u32) -> &'static str {
    let severity = info >> 16;
    let base = info & 0xFFFF;
    match if severity != 0 { severity } else { base } {
        2 => "normal", // AVAILABLE — backends that don't classify report this
        3 => "low",
        4 => "enhancement",
        5 => "normal",
        6 => "bugfix",
        7 => "important",
        8 => "security",
        9 => "blocked",
        26 => "critical",
        27..=30 => "normal", // action codes, not severities
        _ => "unknown",
    }
}

/// PK_STATUS_ENUM_* values reported through the Status property / signals.
fn status_label(status: u32) -> &'static str {
    match status {
        2 => "Waiting…",
        3 => "Starting…",
        4 => "Preparing…",
        5 => "Querying…",
        6 => "Fetching package details…",
        8 => "Removing…",
        9 => "Downloading…",
        10 => "Installing…",
        11 => "Refreshing package lists…",
        12 => "Updating…",
        13 => "Cleaning up…",
        14 => "Resolving dependencies…",
        21 => "Waiting for authentication…",
        22 => "Waiting in queue…",
        _ => "Working…",
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.PackageKit",
    default_service = "org.freedesktop.PackageKit",
    default_path = "/org/freedesktop/PackageKit"
)]
trait PackageKit {
    fn create_transaction(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.PackageKit.Transaction",
    default_service = "org.freedesktop.PackageKit"
)]
trait Transaction {
    fn set_hints(&self, hints: &[&str]) -> zbus::Result<()>;

    fn get_updates(&self, filter: u64) -> zbus::Result<()>;

    fn update_packages(&self, transaction_flags: u64, package_ids: &[&str]) -> zbus::Result<()>;

    fn refresh_cache(&self, force: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn status(&self) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn package(&self, info: u32, package_id: String, summary: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn finished(&self, exit: u32, runtime: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn error_code(&self, code: u32, details: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn require_restart(&self, type_: u32, package_id: String) -> zbus::Result<()>;
}

async fn new_transaction(conn: &zbus::Connection) -> Result<TransactionProxy<'_>> {
    let pk = PackageKitProxy::new(conn).await?;
    let path = pk.create_transaction().await?;
    let txn = TransactionProxy::builder(conn).path(path)?.build().await?;
    // interactive=true lets PackageKit pop the session's polkit agent.
    txn.set_hints(&["interactive=true", "background=false"])
        .await?;
    Ok(txn)
}

pub async fn get_updates(refresh: bool) -> Result<Vec<Update>> {
    let conn = zbus::Connection::system().await?;

    if refresh {
        let txn = new_transaction(&conn).await?;
        let mut finished = txn.receive_finished().await?.fuse();
        let mut errors = txn.receive_error_code().await?.fuse();
        txn.refresh_cache(false).await?;
        loop {
            futures_util::select! {
                sig = errors.next() => {
                    if let Some(sig) = sig {
                        let args = sig.args()?;
                        bail!("PackageKit error {}: {}", args.code(), args.details());
                    }
                }
                _ = finished.next() => break,
            }
        }
    }

    let txn = new_transaction(&conn).await?;

    // Subscribe before calling GetUpdates so no signal slips past us.
    let mut packages = txn.receive_package().await?.fuse();
    let mut finished = txn.receive_finished().await?.fuse();
    let mut errors = txn.receive_error_code().await?.fuse();

    txn.get_updates(FILTER_NONE).await?;

    let mut updates = Vec::new();
    loop {
        futures_util::select! {
            sig = packages.next() => {
                let Some(sig) = sig else { break };
                let args = sig.args()?;
                // package_id is "name;version;arch;origin"
                let mut parts = args.package_id().split(';');
                updates.push(Update {
                    name: parts.next().unwrap_or_default().to_string(),
                    version: parts.next().unwrap_or_default().to_string(),
                    package_id: args.package_id().clone(),
                    summary: args.summary().clone(),
                    severity: severity_from_pk_info(*args.info()).to_string(),
                    info: *args.info(),
                });
            }
            sig = errors.next() => {
                if let Some(sig) = sig {
                    let args = sig.args()?;
                    bail!("PackageKit error {}: {}", args.code(), args.details());
                }
            }
            _ = finished.next() => break,
        }
    }

    updates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(updates)
}

/// Install the given updates, reporting progress through `on_progress`
/// as (percent, detail-line). Percent is `None` while unknown.
pub async fn update_packages(
    package_ids: &[String],
    on_progress: impl Fn(Option<u8>, String),
) -> Result<InstallResult> {
    if package_ids.is_empty() {
        bail!("no packages selected");
    }

    let conn = zbus::Connection::system().await?;
    let txn = new_transaction(&conn).await?;

    let mut packages = txn.receive_package().await?.fuse();
    let mut finished = txn.receive_finished().await?.fuse();
    let mut errors = txn.receive_error_code().await?.fuse();
    let mut restarts = txn.receive_require_restart().await?.fuse();
    let mut percentages = txn.receive_percentage_changed().await.fuse();
    let mut statuses = txn.receive_status_changed().await.fuse();

    let ids: Vec<&str> = package_ids.iter().map(String::as_str).collect();
    txn.update_packages(FLAG_ONLY_TRUSTED, &ids).await?;

    let mut percent: Option<u8> = None;
    let mut needs_restart = false;
    let mut exit_code = 0u32;

    loop {
        futures_util::select! {
            sig = packages.next() => {
                if let Some(sig) = sig {
                    let args = sig.args()?;
                    let name = args.package_id().split(';').next().unwrap_or_default();
                    let verb = status_label(txn.status().await.unwrap_or(0));
                    on_progress(percent, format!("{verb} {name}"));
                }
            }
            change = percentages.next() => {
                if let Some(change) = change {
                    if let Ok(value) = change.get().await {
                        percent = if value == PERCENTAGE_UNKNOWN {
                            None
                        } else {
                            Some(value.min(100) as u8)
                        };
                        on_progress(percent, String::new());
                    }
                }
            }
            change = statuses.next() => {
                if let Some(change) = change {
                    if let Ok(value) = change.get().await {
                        on_progress(percent, status_label(value).to_string());
                    }
                }
            }
            sig = restarts.next() => {
                if let Some(sig) = sig {
                    let args = sig.args()?;
                    if matches!(*args.type_(), RESTART_SYSTEM | RESTART_SECURITY_SYSTEM) {
                        needs_restart = true;
                    }
                }
            }
            sig = errors.next() => {
                if let Some(sig) = sig {
                    let args = sig.args()?;
                    bail!("{}", args.details());
                }
            }
            sig = finished.next() => {
                if let Some(sig) = sig {
                    exit_code = *sig.args()?.exit();
                }
                break;
            }
        }
    }

    if exit_code != EXIT_SUCCESS {
        bail!("update did not complete (PackageKit exit code {exit_code})");
    }
    Ok(InstallResult { needs_restart })
}
