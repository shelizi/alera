//! Explicit, read-only discovery of Managed Model IDs and Agent Personas.
//! Mirrors the supported Classic CLI discovery commands; never launches a
//! Provider simply because a Settings page was opened. No credential data,
//! user args or arbitrary executable flags are accepted from the UI.
use crate::local_agent_detection::{executable_is_runnable, InstalledAgentCli};
use alera_core::process_shell::windowless_shell_command;
use serde_json::Value;
use std::io::{self, Read};
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

const MAX_STDOUT_BYTES: u64 = 1024 * 1024;
const MAX_CHOICES: usize = 128;
const MAX_CHOICE_BYTES: usize = 256;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedChoiceKind {
    Models,
    Personas,
}

/// Fixed, Classic-derived discovery commands. No arbitrary flags, shell
/// commands, profile scripts or login commands can be supplied through UI.
pub fn discovery_arguments(
    agent_id: &str,
    kind: ManagedChoiceKind,
) -> Option<&'static [&'static str]> {
    match (agent_id, kind) {
        ("codex", ManagedChoiceKind::Models) => Some(&["debug", "models"]),
        ("cursor", ManagedChoiceKind::Models) => Some(&["--list-models"]),
        ("agy", ManagedChoiceKind::Models) => Some(&["models"]),
        ("grok", ManagedChoiceKind::Models) => Some(&["models"]),
        ("opencode" | "opencode2", ManagedChoiceKind::Models) => Some(&["models"]),
        ("agy", ManagedChoiceKind::Personas) => Some(&["agents"]),
        ("opencode" | "opencode2", ManagedChoiceKind::Personas) => Some(&["agent", "list"]),
        _ => None,
    }
}

fn allowed_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CHOICE_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':' | b'+')
        })
}

pub fn parse_discovered_choices(
    agent_id: &str,
    kind: ManagedChoiceKind,
    output: &str,
) -> Vec<String> {
    let candidates: Vec<String> = match (agent_id, kind) {
        ("codex", ManagedChoiceKind::Models) => {
            let json = serde_json::from_str::<Value>(output).ok();
            json.as_ref()
                .and_then(|json| json.get("models"))
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            row.get("slug").and_then(Value::as_str).map(str::to_owned)
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        ("cursor", ManagedChoiceKind::Models) => output
            .lines()
            .filter_map(|line| {
                line.trim()
                    .split_once(" - ")
                    .map(|(id, _)| id.trim().to_owned())
            })
            .collect(),
        ("grok", ManagedChoiceKind::Models) => output
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let id = line
                    .strip_prefix("* ")
                    .or_else(|| line.strip_prefix("- "))?;
                Some(id.trim_end_matches(" (default)").trim().to_owned())
            })
            .collect(),
        ("agy", ManagedChoiceKind::Models) => output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.to_ascii_lowercase().starts_with("available model") {
                    return None;
                }
                let id = line
                    .strip_prefix("* ")
                    .or_else(|| line.strip_prefix("- "))
                    .unwrap_or(line);
                Some(id.trim().to_owned())
            })
            .collect(),
        ("opencode" | "opencode2", ManagedChoiceKind::Models) => {
            output.lines().map(|line| line.trim().to_owned()).collect()
        }
        ("agy", ManagedChoiceKind::Personas) => output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.eq_ignore_ascii_case("available agents:") {
                    return None;
                }
                let id = line
                    .strip_prefix("* ")
                    .or_else(|| line.strip_prefix("- "))
                    .unwrap_or(line);
                Some(id.trim().to_owned())
            })
            .collect(),
        ("opencode" | "opencode2", ManagedChoiceKind::Personas) => output
            .lines()
            .filter_map(|line| {
                if !(line.contains("(primary)")
                    || line.contains("(subagent)")
                    || line.contains("(all)"))
                {
                    return None;
                }
                line.split_whitespace().next().map(str::to_owned)
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut unique = Vec::new();
    for id in candidates {
        if allowed_id(&id) && !unique.iter().any(|existing| existing == &id) {
            unique.push(id);
            if unique.len() >= MAX_CHOICES {
                break;
            }
        }
    }
    unique
}

/// Probe only a previously discovered, locally verified installed CLI, with
/// fixed read-only arguments. Call from GPUI's background_spawn on an explicit
/// Refresh button click, never while rendering or automatically on startup.
pub fn discover_choices(
    agent: &InstalledAgentCli,
    kind: ManagedChoiceKind,
) -> io::Result<Vec<String>> {
    let args = discovery_arguments(&agent.id, kind).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "This adapter has no registered discovery command",
        )
    })?;
    if !executable_is_runnable(&agent.executable_path) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Agent CLI not installed; rescan installed Agents",
        ));
    }
    let executable = agent.executable_path.to_string_lossy();
    let arguments: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let mut child = windowless_shell_command(
        &executable,
        &arguments,
        None,
        std::env::var("PATH").ok().as_deref(),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|_| io::Error::other("Agent discovery could not start"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Discovery stdout unavailable"))?;
    let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        output.take(MAX_STDOUT_BYTES + 1).read_to_end(&mut buffer)?;
        Ok(buffer)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break Ok(exit),
            Ok(None) if started.elapsed() < DISCOVERY_TIMEOUT => {
                thread::sleep(Duration::from_millis(20))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Agent discovery timed out",
                ));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(io::Error::other("Agent discovery process failed"));
            }
        }
    };
    // A timed-out Windows shell shim may have a child holding stdout open:
    // never block the UI waiting for the reader after a failed process wait.
    let status = status?;
    let output = reader
        .join()
        .map_err(|_| io::Error::other("Discovery output reader failed"))??;
    if !status.success() {
        return Err(io::Error::other("Agent discovery command failed"));
    }
    if output.len() as u64 > MAX_STDOUT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Agent discovery output too large",
        ));
    }
    let result = parse_discovered_choices(&agent.id, kind, &String::from_utf8_lossy(&output));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_discovery_commands_match_classic_and_never_enable_login_or_launch() {
        assert_eq!(
            discovery_arguments("agy", ManagedChoiceKind::Models),
            Some(&["models"][..])
        );
        assert_eq!(
            discovery_arguments("agy", ManagedChoiceKind::Personas),
            Some(&["agents"][..])
        );
        assert_eq!(
            discovery_arguments("opencode", ManagedChoiceKind::Personas),
            Some(&["agent", "list"][..])
        );
        assert_eq!(
            discovery_arguments("codex", ManagedChoiceKind::Models),
            Some(&["debug", "models"][..])
        );
        assert!(discovery_arguments("claude", ManagedChoiceKind::Personas).is_none());
        assert!(discovery_arguments("arbitrary", ManagedChoiceKind::Models).is_none());
    }

    #[test]
    fn parse_managed_model_and_persona_lists_without_headers_duplicates_or_secrets() {
        assert_eq!(parse_discovered_choices("agy", ManagedChoiceKind::Models,
            "Available Models:\n- gemini-3-pro\n* claude-4.5-sonnet\n- gemini-3-pro\nbad thing\n"),
            ["gemini-3-pro", "claude-4.5-sonnet"]);
        assert_eq!(
            parse_discovered_choices(
                "agy",
                ManagedChoiceKind::Personas,
                "Available agents:\n- plan\n- reviewer\n- plan\n"
            ),
            ["plan", "reviewer"]
        );
        assert_eq!(
            parse_discovered_choices(
                "opencode",
                ManagedChoiceKind::Personas,
                "build (primary)\nreview (subagent)\nnot-a-persona\n"
            ),
            ["build", "review"]
        );
        assert_eq!(
            parse_discovered_choices(
                "opencode",
                ManagedChoiceKind::Models,
                "openai/gpt-5\nopenai/gpt-5\nanthropic/claude-sonnet\n"
            ),
            ["openai/gpt-5", "anthropic/claude-sonnet"]
        );
        assert_eq!(
            parse_discovered_choices(
                "cursor",
                ManagedChoiceKind::Models,
                "sonnet-4 - Sonnet 4 (default)\ngpt-5 - GPT 5\n"
            ),
            ["sonnet-4", "gpt-5"]
        );
        assert_eq!(
            parse_discovered_choices(
                "codex",
                ManagedChoiceKind::Models,
                r#"{"models":[{"slug":"gpt-5.6-sol","display_name":"Sol"},{"slug":"secret token","display_name":"Invalid"}]}"#
            ),
            ["gpt-5.6-sol"]
        );
    }

    /// Uses an entirely fake local Windows CLI. This tests the actual shim,
    /// hard-coded args, process pipes and parser without touching Provider auth.
    #[cfg(windows)]
    #[test]
    fn windows_mock_cli_models_and_personas_are_discovered_end_to_end() {
        let fixture = tempfile::tempdir().expect("disposable CLI fixture");
        let script = fixture.path().join("Mock Agent CLI.cmd");
        std::fs::write(
            &script,
            concat!(
                "@echo off\r\n",
                "if \"%~1\"==\"models\" (\r\n",
                "  echo openai/gpt-5\r\n",
                "  echo anthropic/claude-sonnet\r\n",
                "  exit /b 0\r\n",
                ")\r\n",
                "if \"%~1\"==\"agent\" if \"%~2\"==\"list\" (\r\n",
                "  echo build ^(primary^)\r\n",
                "  echo review ^(subagent^)\r\n",
                "  exit /b 0\r\n",
                ")\r\n",
                "exit /b 5\r\n",
            ),
        )
        .expect("write isolated fake read-only CLI");

        let fake = InstalledAgentCli {
            id: "opencode".into(),
            display_name: "Fake OpenCode".into(),
            executable_path: script,
        };
        assert_eq!(
            discover_choices(&fake, ManagedChoiceKind::Models).unwrap(),
            ["openai/gpt-5", "anthropic/claude-sonnet"]
        );
        assert_eq!(
            discover_choices(&fake, ManagedChoiceKind::Personas).unwrap(),
            ["build", "review"]
        );
    }

    #[test]
    fn unsupported_agent_or_missing_executable_never_starts_discovery() {
        let agent = InstalledAgentCli {
            id: "not-supported".into(),
            display_name: "Other".into(),
            executable_path: "missing-provider-cli.exe".into(),
        };
        assert_eq!(
            discover_choices(&agent, ManagedChoiceKind::Models)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
        let missing = InstalledAgentCli {
            id: "agy".into(),
            ..agent
        };
        assert_eq!(
            discover_choices(&missing, ManagedChoiceKind::Models)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }
}
