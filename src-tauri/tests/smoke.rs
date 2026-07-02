//! Read-only smoke tests against the live system: no root, no writes.
//! Run with `cargo test -- --nocapture` to see what each probe found.

use butterup_lib::{flatpak, health, history, packagekit, snapd, system};

#[tokio::test]
async fn history_parses() {
    let entries = history::get_history(10).await.expect("history");
    println!("history: {} entr(ies)", entries.len());
    for e in entries.iter().take(3) {
        println!(
            "  {} — {} up / {} in / {} rm — {}",
            e.date,
            e.upgraded.len(),
            e.installed.len(),
            e.removed.len(),
            e.command
        );
    }
}

#[tokio::test]
async fn download_size_reads() {
    let size = health::apt_download_size().await;
    println!("apt download size: {size:?}");
}

#[tokio::test]
async fn old_snap_revisions_list() {
    let old = snapd::old_revisions().await.expect("snap list --all");
    println!("old snap revisions: {}", old.len());
    for s in &old {
        println!("  {} rev {} ({})", s.name, s.revision, s.version);
    }
}

#[tokio::test]
async fn system_probe() {
    let info = system::probe().await.expect("system probe");
    println!(
        "system: {} (debian_based={}) packagekit={} pkexec={} snapd={} flatpak={}",
        info.os_name, info.debian_based, info.packagekit, info.pkexec, info.snapd, info.flatpak
    );
    assert!(info.debian_based, "test host is Ubuntu, probe should agree");
    assert!(info.packagekit, "PackageKit is activatable on the test host");
}

#[tokio::test]
async fn apt_updates_list() {
    let updates = packagekit::get_updates(false).await.expect("PackageKit GetUpdates");
    println!("apt: {} pending update(s)", updates.len());
    for u in updates.iter().take(5) {
        println!("  {} {} [{}] — {}", u.name, u.version, u.severity, u.summary);
        assert!(!u.package_id.is_empty());
    }
}

#[tokio::test]
async fn snap_updates_list() {
    let status = snapd::list_updates().await.expect("snap refresh --list");
    println!("snap: available={} {} update(s)", status.available, status.updates.len());
    for u in &status.updates {
        println!("  {} {} ({}) from {}", u.name, u.version, u.size, u.publisher);
    }
}

#[tokio::test]
async fn flatpak_updates_list() {
    let status = flatpak::list_updates().await.expect("flatpak remote-ls");
    println!("flatpak: available={} {} update(s)", status.available, status.updates.len());
}

#[tokio::test]
async fn health_checks_run() {
    let checks = health::run_checks().await.expect("health checks");
    assert_eq!(checks.len(), 8);
    for c in &checks {
        println!("[{}] {} — {}", c.status, c.title, c.detail.replace('\n', " | "));
        assert!(["ok", "warn", "bad", "info", "unknown"].contains(&c.status.as_str()));
    }
}

#[tokio::test]
async fn cleanup_info_reads() {
    let info = health::cleanup_info().await.expect("cleanup info");
    println!(
        "cleanup: {} autoremovable (frees {:?}), cache {} in {} file(s), journal {:?}, {} old snap(s)",
        info.autoremovable.len(),
        info.autoremove_frees,
        info.cache_size,
        info.cache_files,
        info.journal_size,
        info.old_snaps.len()
    );
}
