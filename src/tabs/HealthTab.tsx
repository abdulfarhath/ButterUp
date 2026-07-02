import { useCallback, useEffect, useState } from "react";
import { api, HealthCheck, SystemInfo, useProgress } from "../lib";

const STATUS_LABEL: Record<HealthCheck["status"], string> = {
  ok: "OK",
  warn: "Warning",
  bad: "Problem",
  info: "Info",
  unknown: "Unknown",
};

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "ready"; checks: HealthCheck[] };

interface Props {
  busy: string | null;
  setBusy: (task: string | null) => void;
  system: SystemInfo | null;
}

export default function HealthTab({ busy, setBusy, system }: Props) {
  const canRepair = !system || system.pkexec;
  const [state, setState] = useState<State>({ kind: "loading" });
  const [repairing, setRepairing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const progress = useProgress(repairing ? `action:${repairing}` : "");

  const load = useCallback(async () => {
    setState({ kind: "loading" });
    try {
      setState({ kind: "ready", checks: await api.healthChecks() });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const repair = async (id: string) => {
    setBusy(`action:${id}`);
    setRepairing(id);
    setError(null);
    try {
      await api.runAction(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setRepairing(null);
      progress.reset();
      load();
    }
  };

  return (
    <div className="sections">
      <div className="toolbar">
        <button className="ghost" disabled={busy !== null} onClick={load}>
          ⟳ Re-run checks
        </button>
      </div>

      {error && <p className="inline-error">{error}</p>}
      {!canRepair && (
        <p className="muted">
          Repairs are disabled: pkexec (polkit) isn't installed. Install it with: sudo apt
          install pkexec
        </p>
      )}
      {state.kind === "loading" && <div className="spinner small" aria-label="Loading" />}
      {state.kind === "error" && <p className="inline-error">{state.message}</p>}

      {state.kind === "ready" && (
        <ul className="check-list">
          {state.checks.map((c) => (
            <li key={c.id} className={`card check-row check-${c.status}`}>
              <span className={`status-pill status-${c.status}`}>
                <span className="dot" />
                {STATUS_LABEL[c.status]}
              </span>
              <div className="check-text">
                <span className="check-title">{c.title}</span>
                <span className="check-detail">{c.detail}</span>
                {repairing === c.repair?.id && (
                  <span className="progress-detail">{progress.detail || "Working…"}</span>
                )}
              </div>
              {c.repair && (
                <button
                  className="primary"
                  disabled={busy !== null || !canRepair}
                  onClick={() => repair(c.repair!.id)}
                >
                  {repairing === c.repair.id ? "Repairing…" : c.repair.label}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
