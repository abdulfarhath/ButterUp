//! Probe what this system provides so the UI can adapt: which package
//! managers exist, whether polkit escalation is possible, and whether
//! we're on a Debian-family distro at all.

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    /// PRETTY_NAME from /etc/os-release, e.g. "Ubuntu 26.04 LTS".
    pub os_name: String,
    /// ID or ID_LIKE mentions debian/ubuntu — apt/dpkg tooling applies.
    pub debian_based: bool,
    /// org.freedesktop.PackageKit is reachable or activatable on the system bus.
    pub packagekit: bool,
    pub pkexec: bool,
    pub snapd: bool,
    pub flatpak: bool,
}

fn in_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

fn parse_os_release(content: &str) -> (String, bool) {
    let mut pretty = String::from("Unknown Linux");
    let mut family = String::new();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches('"');
        match key {
            "PRETTY_NAME" => pretty = value.to_string(),
            "ID" | "ID_LIKE" => {
                family.push(' ');
                family.push_str(&value.to_lowercase());
            }
            _ => {}
        }
    }
    let debian_based = family.contains("debian") || family.contains("ubuntu");
    (pretty, debian_based)
}

async fn packagekit_reachable() -> bool {
    let probe = async {
        let conn = zbus::Connection::system().await?;
        let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
        let name = "org.freedesktop.PackageKit".try_into()?;
        Ok::<bool, anyhow::Error>(
            dbus.list_activatable_names().await?.contains(&name)
                || dbus.list_names().await?.contains(&name),
        )
    };
    probe.await.unwrap_or(false)
}

pub async fn probe() -> Result<SystemInfo> {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let (os_name, debian_based) = parse_os_release(&os_release);

    Ok(SystemInfo {
        os_name,
        debian_based,
        packagekit: packagekit_reachable().await,
        pkexec: in_path("pkexec"),
        snapd: std::path::Path::new("/run/snapd.socket").exists() && in_path("snap"),
        flatpak: in_path("flatpak"),
    })
}
