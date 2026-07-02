import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

export interface AptUpdate {
  package_id: string;
  name: string;
  version: string;
  summary: string;
  severity: string;
}

export interface InstallResult {
  needs_restart: boolean;
}

export interface SnapUpdate {
  name: string;
  version: string;
  size: string;
  publisher: string;
}

export interface SnapStatus {
  available: boolean;
  updates: SnapUpdate[];
}

export interface FlatpakUpdate {
  application: string;
  name: string;
  version: string;
}

export interface FlatpakStatus {
  available: boolean;
  updates: FlatpakUpdate[];
}

export interface Repair {
  id: string;
  label: string;
}

export interface HealthCheck {
  id: string;
  title: string;
  status: "ok" | "warn" | "bad" | "info" | "unknown";
  detail: string;
  repair: Repair | null;
}

export interface OldSnap {
  name: string;
  revision: string;
  version: string;
}

export interface CleanupInfo {
  autoremovable: string[];
  autoremove_frees: string;
  cache_size: string;
  cache_files: number;
  journal_size: string;
  old_snaps: OldSnap[];
}

export interface HistoryEntry {
  date: string;
  command: string;
  requested_by: string;
  upgraded: string[];
  installed: string[];
  removed: string[];
}

export interface SystemInfo {
  os_name: string;
  debian_based: boolean;
  packagekit: boolean;
  pkexec: boolean;
  snapd: boolean;
  flatpak: boolean;
}

export const api = {
  systemInfo: () => invoke<SystemInfo>("get_system_info"),
  listApt: (refresh: boolean) => invoke<AptUpdate[]>("list_apt_updates", { refresh }),
  installApt: (packageIds: string[]) =>
    invoke<InstallResult>("install_apt_updates", { packageIds }),
  listSnaps: () => invoke<SnapStatus>("list_snap_updates"),
  refreshSnaps: (names: string[]) => invoke<string>("refresh_snaps", { names }),
  listFlatpaks: () => invoke<FlatpakStatus>("list_flatpak_updates"),
  updateFlatpaks: (applications: string[]) =>
    invoke<string>("update_flatpaks", { applications }),
  healthChecks: () => invoke<HealthCheck[]>("run_health_checks"),
  cleanupInfo: () => invoke<CleanupInfo>("get_cleanup_info"),
  history: () => invoke<HistoryEntry[]>("get_update_history"),
  aptDownloadSize: () => invoke<string>("get_apt_download_size"),
  removeOldSnaps: () => invoke<string>("remove_old_snaps"),
  runAction: (id: string) => invoke<string>("run_action", { id }),
};

interface ProgressPayload {
  task: string;
  percent: number | null;
  detail: string;
}

/** Live progress for one backend task channel ("apt", "snap", "action:…"). */
export function useProgress(task: string) {
  const [percent, setPercent] = useState<number | null>(null);
  const [detail, setDetail] = useState("");

  useEffect(() => {
    if (!task) return;
    const unlisten = listen<ProgressPayload>("task://progress", (e) => {
      if (e.payload.task !== task) return;
      if (e.payload.percent !== null) setPercent(e.payload.percent);
      if (e.payload.detail) setDetail(e.payload.detail);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [task]);

  const reset = useCallback(() => {
    setPercent(null);
    setDetail("");
  }, []);

  return { percent, detail, reset };
}
