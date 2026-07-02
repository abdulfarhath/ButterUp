//! Run maintenance commands, streaming output lines back to the caller.
//! Root actions go through pkexec; polkit shows the authentication
//! dialog through the session's agent.

use anyhow::{bail, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Exit codes pkexec itself uses (as opposed to the wrapped command).
const PKEXEC_AUTH_FAILED: i32 = 127;
const PKEXEC_DISMISSED: i32 = 126;

pub struct CommandOutcome {
    pub code: Option<i32>,
    pub last_line: String,
    pub err_tail: Vec<String>,
}

/// Spawn `cmd` with piped stdout/stderr, forwarding every non-empty
/// line to `on_line`. Both pipes are drained until EOF so the child
/// can never block on a full pipe buffer.
pub async fn stream_command(
    mut cmd: tokio::process::Command,
    on_line: impl Fn(String),
) -> Result<CommandOutcome> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let mut out_lines = BufReader::new(child.stdout.take().expect("stdout piped")).lines();
    let mut err_lines = BufReader::new(child.stderr.take().expect("stderr piped")).lines();

    let mut last = String::new();
    let mut err_tail: Vec<String> = Vec::new();
    let mut out_open = true;
    let mut err_open = true;
    while out_open || err_open {
        tokio::select! {
            line = out_lines.next_line(), if out_open => match line? {
                Some(l) => {
                    let l = l.trim().to_string();
                    if !l.is_empty() {
                        last = l.clone();
                        on_line(l);
                    }
                }
                None => out_open = false,
            },
            line = err_lines.next_line(), if err_open => match line? {
                Some(l) => {
                    let l = l.trim().to_string();
                    if !l.is_empty() {
                        on_line(l.clone());
                        err_tail.push(l);
                        if err_tail.len() > 10 {
                            err_tail.remove(0);
                        }
                    }
                }
                None => err_open = false,
            },
        }
    }

    let status = child.wait().await?;
    Ok(CommandOutcome {
        code: status.code(),
        last_line: last,
        err_tail,
    })
}

pub async fn run_pkexec(argv: &[String], on_line: impl Fn(String)) -> Result<String> {
    if argv.is_empty() {
        bail!("empty command");
    }

    let mut cmd = tokio::process::Command::new("pkexec");
    // pkexec scrubs the environment; `env` re-applies what apt/dpkg need.
    cmd.arg("env")
        .arg("DEBIAN_FRONTEND=noninteractive")
        .arg("LC_ALL=C.UTF-8")
        .args(argv);

    let outcome = stream_command(cmd, on_line).await?;
    match outcome.code {
        Some(0) => Ok(if outcome.last_line.is_empty() {
            "Done".into()
        } else {
            outcome.last_line
        }),
        Some(PKEXEC_DISMISSED) => bail!("Authorization dialog was dismissed"),
        Some(PKEXEC_AUTH_FAILED) => bail!("Not authorized (polkit refused)"),
        code => bail!("command failed (exit {:?}): {}", code, outcome.err_tail.join(" / ")),
    }
}

/// Whitelisted repair / cleanup actions, keyed by a stable id the
/// frontend refers to. Never build these argv from user input.
pub fn action_argv(id: &str) -> Option<Vec<String>> {
    let argv: &[&str] = match id {
        "configure-dpkg" => &["dpkg", "--configure", "-a"],
        "fix-broken" => &["apt-get", "-y", "-f", "install"],
        "autoremove" => &["apt-get", "-y", "--purge", "autoremove"],
        "clean-cache" => &["apt-get", "clean"],
        // Keeps the last week of logs; systemd rotates the rest out.
        "vacuum-journal" => &["journalctl", "--vacuum-time=7d"],
        _ => return None,
    };
    Some(argv.iter().map(|s| s.to_string()).collect())
}
