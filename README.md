# ButterUp 🧈

**One friendly place to update everything on Ubuntu and Debian.**

Keeping a Debian-based system healthy means juggling apt, snap, flatpak, and
more — and when updates go wrong, you're copy-pasting `dpkg` incantations from
forum posts. ButterUp puts it all in one place: see what's pending, update
everything or just what you pick, and detect and repair the common broken
states that make systems misbehave.

## Features

- **Updates** — pending apt (PackageKit over D-Bus), snap, and flatpak
  updates side by side; install everything, just your selection, or
  **security fixes only**, with live progress and the download size shown
  up front. Privilege escalation goes through polkit.
- **Health** — checks for interrupted dpkg operations, half-installed or
  misconfigured packages, broken dependencies, held-back packages, a filling
  `/boot`, kernel build-up, stale package lists, and pending restarts —
  each with a one-click guided repair where one is safe to offer.
- **Cleanup** — remove no-longer-needed packages and old kernels
  (`autoremove --purge`), clear the apt package cache, delete superseded
  snap revisions, and trim the systemd journal — with sizes shown up front.
- **History** — every package change on the system (from apt's own
  transaction log), searchable at a glance: when, what, and who asked.

## Roadmap

- [x] List pending apt updates (PackageKit over D-Bus)
- [x] Install selected / all apt updates with progress
- [x] Snap updates (via `snap refresh`, polkit-authorized)
- [x] Flatpak updates
- [x] Health check: interrupted dpkg, held/broken packages, full /boot
- [x] One-click guided repairs (polkit-authorized)
- [x] Cleanup: autoremove, old kernels, package cache
- [x] .deb packaging (`npm run tauri build`)
- [x] Security-only updates + download size preview
- [x] Update history (apt transaction log)
- [x] Deeper cleanup: old snap revisions, journal vacuum
- [ ] Tray icon + background update notifications
- [ ] Flatpak unused-runtime cleanup
- [ ] PPA publishing

## Install

Download the latest `.deb` from the
[Releases page](https://github.com/abdulfarhath/ButterUp/releases), then:

```bash
sudo apt install ./ButterUp_*.deb
```

Works on Debian 12+, Ubuntu 22.04+ and their derivatives (needs
WebKitGTK 4.1). PackageKit and pkexec are recommended and preinstalled on
standard Ubuntu desktops.

## Tech stack

- **Frontend:** React + TypeScript + Vite
- **Shell:** [Tauri 2](https://v2.tauri.app) (system WebKitGTK, not Electron)
- **Backend:** Rust — `zbus` for PackageKit/D-Bus, snapd REST, polkit

## Developing

System dependencies (Ubuntu/Debian):

```bash
sudo apt install build-essential curl wget file pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev nodejs npm rustc cargo
```

Run in development:

```bash
npm install
npm run tauri dev
```

Build a release binary / .deb:

```bash
npm run tauri build
```

## License

GPL-3.0-or-later
