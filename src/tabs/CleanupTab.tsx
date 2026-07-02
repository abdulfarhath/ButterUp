import { useCallback, useEffect, useState } from "react";
import { api, CleanupInfo, SystemInfo, useProgress } from "../lib";

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "ready"; info: CleanupInfo };

interface Props {
  busy: string | null;
  setBusy: (task: string | null) => void;
  system: SystemInfo | null;
}

export default function CleanupTab({ busy, setBusy, system }: Props) {
  const canRepair = !system || system.pkexec;
  const [state, setState] = useState<State>({ kind: "loading" });
  const [running, setRunning] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const progress = useProgress(running ? `action:${running}` : "");

  const load = useCallback(async () => {
    setState({ kind: "loading" });
    try {
      setState({ kind: "ready", info: await api.cleanupInfo() });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const run = async (id: string, invoke?: () => Promise<string>) => {
    setBusy(`action:${id}`);
    setRunning(id);
    setError(null);
    try {
      await (invoke ? invoke() : api.runAction(id));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setRunning(null);
      progress.reset();
      load();
    }
  };

  if (state.kind === "loading") return <div className="spinner" aria-label="Loading" />;
  if (state.kind === "error")
    return (
      <div className="state">
        <p className="state-title">Couldn't inspect the system</p>
        <p className="state-detail">{state.message}</p>
        <button onClick={load}>Try again</button>
      </div>
    );

  const { info } = state;

  return (
    <div className="sections">
      {error && <p className="inline-error">{error}</p>}
      {!canRepair && (
        <p className="muted">
          Cleanup is disabled: pkexec (polkit) isn't installed. Install it with: sudo apt
          install pkexec
        </p>
      )}

      <section className="card tone-green">
        <header className="card-header">
          <div>
            <h2>Unused packages</h2>
            <span className="card-subtitle">
              {info.autoremovable.length === 0
                ? "nothing to remove"
                : `${info.autoremovable.length} package(s) no longer needed` +
                  (info.autoremove_frees ? ` · frees ${info.autoremove_frees}` : "")}
            </span>
          </div>
          {info.autoremovable.length > 0 && (
            <button
              className="primary"
              disabled={busy !== null || !canRepair}
              onClick={() => run("autoremove")}
            >
              {running === "autoremove" ? "Removing…" : "Remove"}
            </button>
          )}
        </header>
        {running === "autoremove" && (
          <span className="progress-detail">{progress.detail || "Working…"}</span>
        )}
        {info.autoremovable.length > 0 ? (
          <p className="pkg-names">{info.autoremovable.join("  ")}</p>
        ) : (
          <p className="all-clear">
            <span className="dot" /> No orphaned dependencies or old kernels are taking up space
          </p>
        )}
        <p className="muted footnote">
          Runs `apt-get autoremove --purge` — also clears out old kernels safely.
        </p>
      </section>

      <section className="card tone-pink">
        <header className="card-header">
          <div>
            <h2>Package cache</h2>
            <span className="card-subtitle">
              {info.cache_files === 0
                ? "cache is empty"
                : `${info.cache_files} downloaded .deb file(s) · ${info.cache_size}`}
            </span>
          </div>
          {info.cache_files > 0 && (
            <button
              className="primary"
              disabled={busy !== null || !canRepair}
              onClick={() => run("clean-cache")}
            >
              {running === "clean-cache" ? "Cleaning…" : "Clean"}
            </button>
          )}
        </header>
        {running === "clean-cache" && (
          <span className="progress-detail">{progress.detail || "Working…"}</span>
        )}
        {info.cache_files === 0 && (
          <p className="all-clear">
            <span className="dot" /> No cached package downloads
          </p>
        )}
        <p className="muted footnote">
          Downloaded package files apt keeps around after installing. Safe to delete.
        </p>
      </section>

      {info.old_snaps.length > 0 && (
        <section className="card tone-violet">
          <header className="card-header">
            <div>
              <h2>Old snap revisions</h2>
              <span className="card-subtitle">
                {info.old_snaps.length} superseded revision(s) still on disk
              </span>
            </div>
            <button
              className="primary"
              disabled={busy !== null || !canRepair}
              onClick={() => run("remove-old-snaps", api.removeOldSnaps)}
            >
              {running === "remove-old-snaps" ? "Removing…" : "Remove"}
            </button>
          </header>
          {running === "remove-old-snaps" && (
            <span className="progress-detail">{progress.detail || "Working…"}</span>
          )}
          <p className="pkg-names">
            {info.old_snaps.map((s) => `${s.name} (rev ${s.revision})`).join("  ")}
          </p>
          <p className="muted footnote">
            snapd keeps previous versions of every snap for rollback. Removing them is safe and
            often frees gigabytes.
          </p>
        </section>
      )}

      {info.journal_size && (
        <section className="card tone-blue">
          <header className="card-header">
            <div>
              <h2>System logs</h2>
              <span className="card-subtitle">journals take up {info.journal_size}</span>
            </div>
            <button
              className="primary"
              disabled={busy !== null || !canRepair}
              onClick={() => run("vacuum-journal")}
            >
              {running === "vacuum-journal" ? "Trimming…" : "Keep last 7 days"}
            </button>
          </header>
          {running === "vacuum-journal" && (
            <span className="progress-detail">{progress.detail || "Working…"}</span>
          )}
          <p className="muted footnote">
            systemd's journal grows over time. Trimming keeps the last week of logs — enough for
            troubleshooting recent problems.
          </p>
        </section>
      )}
    </div>
  );
}
