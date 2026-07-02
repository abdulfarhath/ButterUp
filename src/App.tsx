import { useEffect, useState } from "react";
import UpdatesTab from "./tabs/UpdatesTab";
import HealthTab from "./tabs/HealthTab";
import CleanupTab from "./tabs/CleanupTab";
import HistoryTab from "./tabs/HistoryTab";
import { api, SystemInfo } from "./lib";

type Tab = "updates" | "health" | "cleanup" | "history";

const TABS: { id: Tab; label: string }[] = [
  { id: "updates", label: "Updates" },
  { id: "health", label: "Health" },
  { id: "cleanup", label: "Cleanup" },
  { id: "history", label: "History" },
];

/** True when the page runs inside the Tauri webview with IPC attached. */
const IN_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export default function App() {
  // Honor #health / #cleanup deep links (also handy for previews).
  const [tab, setTab] = useState<Tab>(() => {
    const hash = window.location.hash.slice(1);
    return TABS.some((t) => t.id === hash) ? (hash as Tab) : "updates";
  });
  // Task id of the operation currently running anywhere in the app —
  // the backend only allows one at a time, so the UI mirrors that.
  const [busy, setBusy] = useState<string | null>(null);
  const [needsRestart, setNeedsRestart] = useState(false);
  const [system, setSystem] = useState<SystemInfo | null>(null);

  useEffect(() => {
    if (IN_TAURI) api.systemInfo().then(setSystem, () => setSystem(null));
  }, []);

  if (!IN_TAURI) {
    return (
      <div className="app">
        <div className="state">
          <p className="state-emoji">🧈</p>
          <p className="state-title">This window isn't connected to the ButterUp backend</p>
          <p className="state-detail">
            You're viewing the interface in a regular browser, so it can't talk to the system.
            Launch the desktop app instead: run <code>npm run tauri dev</code> from the project
            folder (not <code>npm run dev</code>), or start the installed ButterUp application.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="header">
        <div className="header-title">
          <span className="brand-mark">🧈</span>
          <div>
            <h1>ButterUp</h1>
            <span className="subtitle">
              {system ? system.os_name : "Updates, health and cleanup in one place"}
            </span>
          </div>
        </div>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`tab${tab === t.id ? " active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      {system && !system.debian_based && (
        <div className="banner warning">
          ⚠️ {system.os_name} doesn't look Debian-based. ButterUp manages apt/dpkg systems —
          most features won't work here.
        </div>
      )}

      {needsRestart && (
        <div className="banner">
          🔁 A restart is needed to finish applying updates.
          <button className="banner-dismiss" onClick={() => setNeedsRestart(false)} title="Dismiss">
            ✕
          </button>
        </div>
      )}

      <main className="content">
        {/* Tabs stay mounted so running operations keep their progress UI. */}
        <div hidden={tab !== "updates"}>
          <UpdatesTab
            busy={busy}
            setBusy={setBusy}
            system={system}
            onNeedsRestart={() => setNeedsRestart(true)}
          />
        </div>
        <div hidden={tab !== "health"}>
          <HealthTab busy={busy} setBusy={setBusy} system={system} />
        </div>
        <div hidden={tab !== "cleanup"}>
          <CleanupTab busy={busy} setBusy={setBusy} system={system} />
        </div>
        <div hidden={tab !== "history"}>
          <HistoryTab />
        </div>
      </main>
    </div>
  );
}
