import { useCallback, useEffect, useState } from "react";
import { api, HistoryEntry } from "../lib";

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "ready"; entries: HistoryEntry[] };

export default function HistoryTab() {
  const [state, setState] = useState<State>({ kind: "loading" });

  const load = useCallback(async () => {
    setState({ kind: "loading" });
    try {
      setState({ kind: "ready", entries: await api.history() });
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (state.kind === "loading") return <div className="spinner" aria-label="Loading" />;
  if (state.kind === "error")
    return (
      <div className="state">
        <p className="state-title">Couldn't read the update history</p>
        <p className="state-detail">{state.message}</p>
        <button onClick={load}>Try again</button>
      </div>
    );

  return (
    <div className="sections">
      <div className="toolbar">
        <button className="ghost" onClick={load}>
          ⟳ Reload
        </button>
      </div>

      {state.entries.length === 0 && (
        <p className="muted">No package changes recorded yet in apt's history log.</p>
      )}

      {state.entries.map((e, i) => (
        <details key={`${e.date}-${i}`} className="card history-entry span-all">
          <summary className="history-summary">
            <span className="history-date">{e.date}</span>
            <span className="history-counts">
              {e.upgraded.length > 0 && (
                <span className="badge badge-important">{e.upgraded.length} upgraded</span>
              )}
              {e.installed.length > 0 && (
                <span className="badge badge-bugfix">{e.installed.length} installed</span>
              )}
              {e.removed.length > 0 && (
                <span className="badge badge-security">{e.removed.length} removed</span>
              )}
            </span>
            <span className="history-cmd" title={e.command}>
              {e.command}
            </span>
          </summary>
          <div className="history-body">
            {(
              [
                ["Upgraded", e.upgraded],
                ["Installed", e.installed],
                ["Removed", e.removed],
              ] as const
            ).map(
              ([label, items]) =>
                items.length > 0 && (
                  <div key={label}>
                    <p className="history-label">{label}</p>
                    <p className="pkg-names">{items.join("\n")}</p>
                  </div>
                )
            )}
            {e.requested_by && <p className="footnote muted">Requested by {e.requested_by}</p>}
          </div>
        </details>
      ))}
    </div>
  );
}
