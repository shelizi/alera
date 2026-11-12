//! Local, read-only installed Agent CLI discovery for the Native New Tab menu.
//! Classic uses the descriptor registry and PATH/PATHEXT, independently of Host
//! Agent Profiles. Never launch an Agent or Host while merely listing commands.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledAgentCli {
    pub id: String,
    pub display_name: String,
    /// Concrete path that passed local existence/health checks; not a shell line.
    pub executable_path: PathBuf,
}

/// Probe installed, spawnable Agent adapters independently of Host Agent
/// Profiles. The caller must run this on a background thread, never in paint.
pub fn detect_installed_agent_clis(
    configured_paths: &HashMap<String, String>,
) -> Vec<InstalledAgentCli> {
    let environment = std::env::vars().collect::<HashMap<_, _>>();
    detect_with(
        &environment,
        configured_paths,
        cfg!(windows),
        executable_is_runnable,
        healthy_agy_version,
    )
}

/// Installed CLI presence and Host profile configuration are independent.
/// A matching Host profile is the only currently supported Native launch route.
/// Prefer the user's selected/default profile when more than one matches.
pub fn matching_host_profile_for_agent<'a>(
    agent_id: &str,
    profiles: &'a [crate::agent_profiles::AgentProfileRow],
    preferred_profile_id: Option<&str>,
) -> Option<&'a crate::agent_profiles::AgentProfileRow> {
    let expected = alera_core::agent_descriptor::canonical_agent_id(agent_id)?;
    let matches = |profile: &&crate::agent_profiles::AgentProfileRow| {
        alera_core::agent_descriptor::canonical_agent_id(&profile.agent_type) == Some(expected)
    };
    profiles
        .iter()
        .filter(matches)
        .find(|profile| Some(profile.id.as_str()) == preferred_profile_id)
        .or_else(|| {
            profiles.iter().find(|profile| {
                alera_core::agent_descriptor::canonical_agent_id(&profile.agent_type)
                    == Some(expected)
            })
        })
}

fn executable_is_runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Classic only requires a healthy --version for AGY, not the other adapters.
/// Use the shared windowless shell process policy for .cmd / .bat shims.
fn healthy_agy_version(executable: &Path) -> bool {
    use alera_core::process_shell::windowless_shell_command;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let executable = executable.to_string_lossy();
    let mut child = match windowless_shell_command(
        &executable,
        &["--version".into()],
        None,
        std::env::var("PATH").ok().as_deref(),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn path_environment<'a>(
    environment: &'a HashMap<String, String>,
    key: &str,
    windows: bool,
) -> Option<&'a str> {
    if windows {
        environment
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    } else {
        environment.get(key).map(String::as_str)
    }
}

fn resolve_from_path<F: Fn(&Path) -> bool>(
    command: &str,
    environment: &HashMap<String, String>,
    windows: bool,
    exists: &F,
) -> Option<PathBuf> {
    if command.is_empty() || command.contains(['/', '\\']) {
        return None;
    }
    let path = path_environment(environment, "PATH", windows)?;
    let candidates = if windows {
        let ext = path_environment(environment, "PATHEXT", windows)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(".COM;.EXE;.BAT;.CMD");
        let extensions = ext
            .split(';')
            .map(str::trim)
            .filter(|ext| ext.starts_with('.') && ext.len() > 1)
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        let already_extended = extensions
            .iter()
            .any(|ext| command.to_ascii_lowercase().ends_with(ext));
        if already_extended {
            vec![command.to_owned()]
        } else {
            let mut names = extensions
                .iter()
                .map(|ext| format!("{command}{ext}"))
                .collect::<Vec<_>>();
            names.push(command.to_owned());
            names
        }
    } else {
        vec![command.to_owned()]
    };
    for segment in path.split(if windows { ';' } else { ':' }) {
        let segment = segment.trim();
        let segment = if windows {
            segment
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(segment)
                .trim()
        } else {
            segment
        };
        if segment.is_empty() {
            continue;
        }
        for candidate in &candidates {
            let resolved = if windows {
                PathBuf::from(format!("{segment}\\{candidate}"))
            } else {
                Path::new(segment).join(candidate)
            };
            if exists(&resolved) {
                return Some(resolved);
            }
        }
    }
    None
}

fn detect_with<F: Fn(&Path) -> bool, V: Fn(&Path) -> bool>(
    environment: &HashMap<String, String>,
    configured_paths: &HashMap<String, String>,
    windows: bool,
    exists: F,
    version_healthy: V,
) -> Vec<InstalledAgentCli> {
    // The Rust descriptor registry is the source that also generates the
    // Classic Dart table; it guarantees identical agent order and labels.
    alera_core::agent_descriptor::AGENT_DESCRIPTORS
        .iter()
        .filter_map(|descriptor| {
            let executable = match configured_paths
                .get(descriptor.id)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                Some(override_path) => {
                    let path = PathBuf::from(override_path);
                    exists(&path).then_some(path)
                }
                None => {
                    resolve_from_path(descriptor.default_command, environment, windows, &exists)
                }
            }?;
            if descriptor.id == "agy" && !version_healthy(&executable) {
                return None;
            }
            Some(InstalledAgentCli {
                id: descriptor.id.to_owned(),
                display_name: descriptor.display_name.to_owned(),
                executable_path: executable,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn probe(
        path: &str,
        existing: &[&str],
        overrides: &[(&str, &str)],
        agy_healthy: bool,
    ) -> Vec<InstalledAgentCli> {
        let environment = HashMap::from([
            ("Path".to_string(), path.to_string()),
            ("PATHEXT".to_string(), ".COM;.EXE;.BAT;.CMD".to_string()),
        ]);
        let found: HashSet<_> = existing.iter().map(|s| PathBuf::from(s)).collect();
        let overrides = overrides
            .iter()
            .map(|(id, path)| (id.to_string(), path.to_string()))
            .collect::<HashMap<_, _>>();
        detect_with(
            &environment,
            &overrides,
            true,
            |path| found.contains(path),
            |_path| agy_healthy,
        )
    }

    #[test]
    fn native_detects_locally_installed_clis_even_without_host_agent_profiles() {
        let rows = probe(
            r"C:\tools;C:\other",
            &[
                r"C:\tools\claude.cmd",
                r"C:\other\codex.exe",
                r"C:\other\grok.exe",
            ],
            &[],
            false,
        );
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["codex", "claude", "grok",]
        );
        assert_eq!(rows[1].display_name, "Claude Code");
    }

    #[test]
    fn windows_path_is_case_insensitive_and_quoted_segments_are_supported() {
        let rows = probe(
            r#""C:\Program Files\Agents"; C:\other"#,
            &[
                r"C:\Program Files\Agents\copilot.cmd",
                r"C:\other\devin.exe",
            ],
            &[],
            false,
        );
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["copilot", "devin",]
        );
    }

    #[test]
    fn explicit_executable_outside_path_is_used_and_invalid_override_fails_closed() {
        let rows = probe(
            r"C:\tools",
            &[r"C:\tools\devin.exe", r"D:\Agents\Devin\devin.exe"],
            &[("devin", r"D:\Agents\Devin\devin.exe")],
            false,
        );
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["devin"]
        );
        assert!(probe(
            r"C:\tools",
            &[r"C:\tools\devin.exe"],
            &[("devin", r"D:\missing\devin.exe")],
            false,
        )
        .is_empty());
    }

    #[test]
    fn agy_requires_successful_version_probe_before_menu_entry_is_shown() {
        assert!(probe(r"C:\tools", &[r"C:\tools\agy.cmd"], &[], false).is_empty());
        assert_eq!(
            probe(r"C:\tools", &[r"C:\tools\agy.cmd"], &[], true)[0].id,
            "agy",
        );
    }

    #[test]
    fn installed_agent_selects_matching_host_profile_without_cross_agent_fallback() {
        use crate::agent_profiles::AgentProfileRow;
        let rows = vec![
            AgentProfileRow {
                id: "claude_p1".into(),
                name: "Claude Default".into(),
                agent_type: "claude".into(),
                managed: true,
            },
            AgentProfileRow {
                id: "claude_p2".into(),
                name: "Claude Review".into(),
                agent_type: "claude".into(),
                managed: false,
            },
            AgentProfileRow {
                id: "codex_p1".into(),
                name: "Codex".into(),
                agent_type: "codex".into(),
                managed: true,
            },
        ];
        assert_eq!(
            matching_host_profile_for_agent("claude", &rows, Some("claude_p2"))
                .map(|p| p.id.as_str()),
            Some("claude_p2")
        );
        assert_eq!(
            matching_host_profile_for_agent("claude", &rows, Some("missing"))
                .map(|p| p.id.as_str()),
            Some("claude_p1")
        );
        assert_eq!(
            matching_host_profile_for_agent("codex", &rows, Some("claude_p2"))
                .map(|p| p.id.as_str()),
            Some("codex_p1")
        );
        assert!(matching_host_profile_for_agent("grok", &rows, None).is_none());
        assert!(matching_host_profile_for_agent("invalid-agent", &rows, None).is_none());
    }

    #[test]
    fn missing_path_and_unrecognized_scripts_never_create_fake_agents() {
        assert!(probe("", &[r"C:\tools\codex.ps1"], &[], false).is_empty());
        assert!(probe(r"C:\tools", &[r"C:\tools\codex.ps1"], &[], false).is_empty());
    }
}
