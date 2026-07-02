pub mod flatpak;
pub mod health;
pub mod history;
pub mod packagekit;
pub mod privileged;
pub mod snapd;
pub mod system;

use tauri::{AppHandle, Emitter, Manager, State};

use packagekit::{InstallResult, Update};

/// Serialized to the frontend on the `task://progress` event channel.
#[derive(Clone, serde::Serialize)]
struct Progress {
    task: String,
    percent: Option<u8>,
    detail: String,
}

fn emit_progress(app: &AppHandle, task: &str, percent: Option<u8>, detail: String) {
    let _ = app.emit(
        "task://progress",
        Progress { task: task.into(), percent, detail },
    );
}

/// Only one mutating operation (install, refresh, repair…) may run at a
/// time; concurrent apt/dpkg invocations would fight over locks anyway.
struct OpGuard(tokio::sync::Mutex<()>);

async fn guarded<T>(
    guard: &OpGuard,
    op: impl std::future::Future<Output = anyhow::Result<T>>,
) -> Result<T, String> {
    let Ok(_lock) = guard.0.try_lock() else {
        return Err("Another operation is already running".into());
    };
    op.await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_system_info() -> Result<system::SystemInfo, String> {
    system::probe().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_apt_updates(refresh: bool) -> Result<Vec<Update>, String> {
    packagekit::get_updates(refresh).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_apt_updates(
    app: AppHandle,
    guard: State<'_, OpGuard>,
    package_ids: Vec<String>,
) -> Result<InstallResult, String> {
    guarded(&guard, async {
        packagekit::update_packages(&package_ids, |percent, detail| {
            emit_progress(&app, "apt", percent, detail);
        })
        .await
    })
    .await
}

#[tauri::command]
async fn list_snap_updates() -> Result<snapd::SnapStatus, String> {
    snapd::list_updates().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn refresh_snaps(
    app: AppHandle,
    guard: State<'_, OpGuard>,
    names: Vec<String>,
) -> Result<String, String> {
    guarded(&guard, async {
        snapd::refresh(&names, |line| emit_progress(&app, "snap", None, line)).await
    })
    .await
}

#[tauri::command]
async fn list_flatpak_updates() -> Result<flatpak::FlatpakStatus, String> {
    flatpak::list_updates().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_flatpaks(
    app: AppHandle,
    guard: State<'_, OpGuard>,
    applications: Vec<String>,
) -> Result<String, String> {
    guarded(&guard, async {
        flatpak::update(&applications, |line| emit_progress(&app, "flatpak", None, line)).await
    })
    .await
}

#[tauri::command]
async fn run_health_checks() -> Result<Vec<health::Check>, String> {
    health::run_checks().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_update_history() -> Result<Vec<history::HistoryEntry>, String> {
    history::get_history(60).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_apt_download_size() -> Result<String, String> {
    Ok(health::apt_download_size().await)
}

#[tauri::command]
async fn remove_old_snaps(
    app: AppHandle,
    guard: State<'_, OpGuard>,
) -> Result<String, String> {
    guarded(&guard, async {
        snapd::remove_old_revisions(|line| {
            emit_progress(&app, "action:remove-old-snaps", None, line)
        })
        .await
    })
    .await
}

#[tauri::command]
async fn get_cleanup_info() -> Result<health::CleanupInfo, String> {
    health::cleanup_info().await.map_err(|e| e.to_string())
}

/// Run a whitelisted repair/cleanup action (see privileged::action_argv).
#[tauri::command]
async fn run_action(
    app: AppHandle,
    guard: State<'_, OpGuard>,
    id: String,
) -> Result<String, String> {
    let argv = privileged::action_argv(&id).ok_or_else(|| format!("unknown action: {id}"))?;
    let task = format!("action:{id}");
    guarded(&guard, async {
        privileged::run_pkexec(&argv, |line| emit_progress(&app, &task, None, line)).await
    })
    .await
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(OpGuard(tokio::sync::Mutex::new(())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            list_apt_updates,
            install_apt_updates,
            list_snap_updates,
            refresh_snaps,
            list_flatpak_updates,
            update_flatpaks,
            run_health_checks,
            get_cleanup_info,
            get_update_history,
            get_apt_download_size,
            remove_old_snaps,
            run_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
