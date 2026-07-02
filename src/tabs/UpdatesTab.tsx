import { useCallback, useEffect, useState } from "react";
import { api, SystemInfo, useProgress } from "../lib";

const SEVERITY_ORDER: Record<string, number> = {
  critical: 0,
  security: 1,
  important: 2,
  bugfix: 3,
  normal: 4,
  enhancement: 5,
  low: 6,
  blocked: 7,
  unknown: 8,
};

interface SectionRow {
  id: string;
  name: string;
  detail: string;
  badge?: string;
  meta?: string;
}

type SectionData =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "unavailable"; message: string }
  | { kind: "ready"; rows: SectionRow[] };

interface Props {
  busy: string | null;
  setBusy: (task: string | null) => void;
  system: SystemInfo | null;
  onNeedsRestart: () => void;
}

export default function UpdatesTab({ busy, setBusy, system, onNeedsRestart }: Props) {
  const [apt, setApt] = useState<SectionData>({ kind: "loading" });
  const [snap, setSnap] = useState<SectionData>({ kind: "loading" });
  const [flatpak, setFlatpak] = useState<SectionData>({ kind: "loading" });
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [aptSize, setAptSize] = useState("");

  const loadApt = useCallback(async (refresh: boolean) => {
    setApt({ kind: "loading" });
    setAptSize("");
    api.aptDownloadSize().then(setAptSize, () => {});
    try {
      const updates = await api.listApt(refresh);
      updates.sort(
        (a, b) =>
          (SEVERITY_ORDER[a.severity] ?? 9) - (SEVERITY_ORDER[b.severity] ?? 9) ||
          a.name.localeCompare(b.name)
      );
      setApt({
        kind: "ready",
        rows: updates.map((u) => ({
          id: u.package_id,
          name: u.name,
          detail: u.summary,
          badge: u.severity === "unknown" ? undefined : u.severity,
          meta: u.version,
        })),
      });
    } catch (e) {
      setApt({ kind: "error", message: String(e) });
    }
  }, []);

  const loadSnap = useCallback(async () => {
    setSnap({ kind: "loading" });
    try {
      const status = await api.listSnaps();
      if (!status.available) {
        setSnap({ kind: "unavailable", message: "snapd isn't running on this system." });
        return;
      }
      setSnap({
        kind: "ready",
        rows: status.updates.map((u) => ({
          id: u.name,
          name: u.name,
          detail: `${u.publisher} · ${u.size}`,
          meta: u.version,
        })),
      });
    } catch (e) {
      setSnap({ kind: "error", message: String(e) });
    }
  }, []);

  const loadFlatpak = useCallback(async () => {
    setFlatpak({ kind: "loading" });
    try {
      const status = await api.listFlatpaks();
      if (!status.available) {
        setFlatpak({ kind: "unavailable", message: "Flatpak isn't installed on this system." });
        return;
      }
      setFlatpak({
        kind: "ready",
        rows: status.updates.map((u) => ({
          id: u.application,
          name: u.name || u.application,
          detail: u.application,
          meta: u.version,
        })),
      });
    } catch (e) {
      setFlatpak({ kind: "error", message: String(e) });
    }
  }, []);

  useEffect(() => {
    loadApt(false);
    loadSnap();
    loadFlatpak();
  }, [loadApt, loadSnap, loadFlatpak]);

  const run = async (
    task: string,
    fn: () => Promise<void>,
    reload: () => void
  ) => {
    setBusy(task);
    setErrors((e) => ({ ...e, [task]: "" }));
    try {
      await fn();
    } catch (e) {
      setErrors((prev) => ({ ...prev, [task]: String(e) }));
    } finally {
      setBusy(null);
      reload();
    }
  };

  return (
    <div className="sections cols-3">
      <div className="toolbar">
        <button
          className="ghost"
          disabled={busy !== null}
          onClick={() => {
            loadApt(true);
            loadSnap();
            loadFlatpak();
          }}
        >
          ⟳ Check again
        </button>
      </div>

      <Section
        title="System packages"
        subtitle={aptSize ? `apt / PackageKit · ${aptSize} to download` : "apt / PackageKit"}
        task="apt"
        tone="amber"
        securityFirst
        data={
          system && !system.packagekit
            ? {
                kind: "unavailable",
                message:
                  "PackageKit isn't available on this system. Install it with: sudo apt install packagekit",
              }
            : apt
        }
        busy={busy}
        error={errors["apt"]}
        onInstall={(ids) =>
          run(
            "apt",
            async () => {
              const result = await api.installApt(ids);
              if (result.needs_restart) onNeedsRestart();
            },
            () => loadApt(false)
          )
        }
      />

      <Section
        title="Snaps"
        subtitle="snap store"
        task="snap"
        tone="violet"
        data={snap}
        busy={busy}
        error={errors["snap"]}
        allMeansEmpty
        onInstall={(ids) => run("snap", async () => void (await api.refreshSnaps(ids)), loadSnap)}
      />

      <Section
        title="Flatpak apps"
        subtitle="flathub & friends"
        task="flatpak"
        tone="cyan"
        data={flatpak}
        busy={busy}
        error={errors["flatpak"]}
        allMeansEmpty
        onInstall={(ids) =>
          run("flatpak", async () => void (await api.updateFlatpaks(ids)), loadFlatpak)
        }
      />
    </div>
  );
}

function Section(props: {
  title: string;
  subtitle: string;
  task: string;
  data: SectionData;
  busy: string | null;
  error?: string;
  /** When updating everything, send an empty list ("update all") instead of every id. */
  allMeansEmpty?: boolean;
  /** Offer a "security only" button when security/critical rows exist. */
  securityFirst?: boolean;
  /** Accent color class for the card (amber, violet, cyan…). */
  tone?: string;
  onInstall: (ids: string[]) => void;
}) {
  const {
    title, subtitle, task, data, busy, error, allMeansEmpty, securityFirst, tone, onInstall,
  } = props;
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const progress = useProgress(task);
  const running = busy === task;
  const anyBusy = busy !== null;

  useEffect(() => {
    setSelected(new Set());
    if (!running) progress.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);

  const rows = data.kind === "ready" ? data.rows : [];

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const allSelected = rows.length > 0 && selected.size === rows.length;
  const toggleAll = () =>
    setSelected(allSelected ? new Set() : new Set(rows.map((r) => r.id)));

  const hasBadges = rows.some((r) => r.badge);
  const securityIds = securityFirst
    ? rows.filter((r) => r.badge === "security" || r.badge === "critical").map((r) => r.id)
    : [];

  const install = () => {
    if (selected.size > 0 && selected.size < rows.length) {
      onInstall([...selected]);
    } else {
      onInstall(allMeansEmpty ? [] : rows.map((r) => r.id));
    }
  };

  const buttonLabel =
    selected.size > 0 && selected.size < rows.length
      ? `Update selected (${selected.size})`
      : "Update all";

  return (
    <section className={`card${tone ? ` tone-${tone}` : ""}`}>
      <header className="card-header">
        <div>
          <h2>{title}</h2>
          <span className="card-subtitle">
            {data.kind === "ready" &&
              (rows.length === 0 ? "up to date" : `${rows.length} pending · ${subtitle}`)}
            {data.kind === "loading" && "checking…"}
            {(data.kind === "error" || data.kind === "unavailable") && subtitle}
          </span>
        </div>
        {data.kind === "ready" && rows.length > 0 && (
          <div className="card-actions">
            {securityIds.length > 0 && (
              <button
                className="ghost security"
                disabled={anyBusy}
                onClick={() => onInstall(securityIds)}
                title="Install only security and critical updates"
              >
                Security only ({securityIds.length})
              </button>
            )}
            <button className="primary" disabled={anyBusy} onClick={install}>
              {running ? "Updating…" : buttonLabel}
            </button>
          </div>
        )}
      </header>

      {running && (
        <div className="progress">
          <div className={`progress-bar${progress.percent === null ? " indeterminate" : ""}`}>
            <div
              className="progress-fill"
              style={progress.percent !== null ? { width: `${progress.percent}%` } : undefined}
            />
          </div>
          <span className="progress-detail">{progress.detail || "Working…"}</span>
        </div>
      )}

      {error && <p className="inline-error">{error}</p>}

      {data.kind === "loading" && <div className="spinner small" aria-label="Loading" />}
      {data.kind === "error" && <p className="inline-error">{data.message}</p>}
      {data.kind === "unavailable" && <p className="muted">{data.message}</p>}
      {data.kind === "ready" && rows.length === 0 && (
        <p className="all-clear">
          <span className="dot" /> Everything is up to date
        </p>
      )}

      {data.kind === "ready" && rows.length > 0 && (
        <div className="table-wrap">
          <table className="pkg-table">
            <thead>
              <tr>
                <th className="col-check">
                  <input
                    type="checkbox"
                    checked={allSelected}
                    disabled={anyBusy}
                    onChange={toggleAll}
                    title="Select all"
                  />
                </th>
                <th>Package</th>
                {hasBadges && <th className="col-sev">Severity</th>}
                <th className="col-ver">Version</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr
                  key={r.id}
                  className={selected.has(r.id) ? "selected" : undefined}
                  onClick={() => !anyBusy && toggle(r.id)}
                >
                  <td className="col-check">
                    <input
                      type="checkbox"
                      checked={selected.has(r.id)}
                      disabled={anyBusy}
                      onChange={() => toggle(r.id)}
                      onClick={(e) => e.stopPropagation()}
                    />
                  </td>
                  <td className="col-name" title={r.detail}>
                    <span className="pkg-name">{r.name}</span>
                    <span className="pkg-sub">{r.detail}</span>
                  </td>
                  {hasBadges && (
                    <td className="col-sev">
                      {r.badge && <span className={`badge badge-${r.badge}`}>{r.badge}</span>}
                    </td>
                  )}
                  <td className="col-ver">{r.meta}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
