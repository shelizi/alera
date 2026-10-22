use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alera_core::runtime::RuntimeAgentStatusHookSettings;
use serde_json::{json, Map, Value};

#[cfg(windows)]
#[path = "integration_config_agy_windows.rs"]
mod agy_windows;
#[path = "integration_config_ccs.rs"]
mod ccs;
#[path = "integration_config_codex.rs"]
mod codex;
#[path = "integration_config_codex_trust.rs"]
mod codex_hook_trust;
#[path = "integration_config_cursor.rs"]
mod cursor;
#[cfg(any(windows, test))]
#[path = "integration_config_devin_paths.rs"]
mod devin_paths;
#[path = "integration_config_dispatch.rs"]
mod dispatch;
#[path = "integration_config_user_hooks.rs"]
mod user_hooks;

pub use dispatch::prepare_enabled_integrations;

const MANAGED_MARKER: &str = "alera-runtime-agent-hook";
const LEGACY_MANAGED_MARKERS: [&str; 10] = [
    "alera-codex-hook.",
    "alera-claude-hook.",
    "alera-copilot-hook.",
    "alera-cursor-hook.",
    // Antigravity is the one agent whose desktop installer also writes per-event
    // Windows wrappers (`alera-agy-stop.cmd` and friends), so the marker has to
    // cover the whole `alera-agy-*` family rather than just the core script.
    "alera-agy-",
    "alera-devin-hook.",
    "alera-opencode-hook.",
    "alera-pi-hook.",
    "alera-amp-hook.",
    "alera-grok-hook.",
];

/// Host start: clear what a previous run left behind, then reconcile.
///
/// The clearing has to happen here and only here. The host owns no PTY yet, so
/// this is the one moment a per-session leftover is provably dead, and it is
/// also the only point that runs when every hook toggle is off.
pub fn start_agent_integrations(
    runtime_dir: &Path,
    settings: &RuntimeAgentStatusHookSettings,
) -> Vec<String> {
    let mut warnings = match cursor::clear_stale_state(runtime_dir) {
        Ok(()) => Vec::new(),
        Err(error) => vec![format!("Cursor: {error}")],
    };
    warnings.extend(reconcile_agent_integrations(runtime_dir, settings));
    warnings
}

pub fn reconcile_agent_integrations(
    runtime_dir: &Path,
    settings: &RuntimeAgentStatusHookSettings,
) -> Vec<String> {
    let mut environment = std::env::vars().collect::<BTreeMap<_, _>>();
    prepare_enabled_integrations(runtime_dir, None, settings, &mut environment)
}

fn prepare_claude(
    runtime_dir: &Path,
    script: &Path,
    environment: &BTreeMap<String, String>,
) -> anyhow::Result<(PathBuf, Vec<String>)> {
    let home = home_dir()?;
    let source = home.join(".claude");
    let runtime_home = runtime_dir.join("agent-runtime-homes/claude/home");
    std::fs::create_dir_all(&runtime_home)?;
    if source.exists() {
        for entry in std::fs::read_dir(&source)? {
            let entry = entry?;
            if entry.file_name() != "settings.json" {
                link_if_present(&entry.path(), &runtime_home.join(entry.file_name()));
            }
        }
    }
    let mut settings = read_json_object(&source.join("settings.json"))?.unwrap_or_default();
    install_claude_hooks_into(&mut settings, script);
    write_json_object(&runtime_home.join("settings.json"), &settings)?;
    // CCS overrides CLAUDE_CONFIG_DIR to its own instance, so the overlay above
    // never reaches those sessions. The user's settings.json is what every
    // instance symlinks to, and the only file Claude reads from a config
    // directory. Keep the overlay even when that write fails.
    let mut warnings = Vec::new();
    if let Err(error) = user_hooks::install_claude_user_hooks(&home, script) {
        warnings.push(error.to_string());
    }
    // Older Alera versions wrote into the CCS instances themselves. Claude never
    // read those files; strip them so nothing double-POSTs.
    if let Err(error) = ccs::remove_ccs_claude_hooks(&home, environment) {
        warnings.push(error.to_string());
    }
    // Install after CCS cleanup so an independent instance keeps its active hooks.
    if let Some(directory) = environment.get("CLAUDE_CONFIG_DIR") {
        if let Err(error) = user_hooks::install_claude_config_hooks(Path::new(directory), script) {
            warnings.push(error.to_string());
        }
    }
    Ok((runtime_home, warnings))
}

pub(super) const CLAUDE_HOOK_EVENTS: &[(&str, Option<&str>)] = &[
    ("UserPromptSubmit", None),
    ("Stop", None),
    ("StopFailure", None),
    ("PreToolUse", Some("*")),
    ("PostToolUse", Some("*")),
    ("PostToolUseFailure", Some("*")),
    ("PermissionRequest", Some("*")),
];

pub(super) fn install_claude_hooks_into(settings: &mut Map<String, Value>, script: &Path) {
    let hooks = object_field(settings, "hooks");
    for (event, matcher) in CLAUDE_HOOK_EVENTS {
        let command = claude_managed_command(script, event);
        let mut definitions = clean_managed_definitions(hooks.remove(*event));
        definitions.push(managed_hook_definition(*matcher, &command));
        hooks.insert((*event).to_string(), Value::Array(definitions));
    }
}

fn install_copilot(script: &Path) -> anyhow::Result<()> {
    let home = env_path("COPILOT_HOME").unwrap_or(home_dir()?.join(".copilot"));
    let path = home.join("hooks/alera.json");
    let events = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "SubagentStart",
        "SubagentStop",
        "PreCompact",
        "Stop",
        "ErrorOccurred",
        "PermissionRequest",
        "Notification",
    ];
    let hooks = events
        .into_iter()
        .map(|event| {
            (
                event.to_string(),
                json!([{ "type": "command", "bash": managed_command(script, "copilot", event), "timeoutSec": 5 }]),
            )
        })
        .collect::<Map<_, _>>();
    write_json_object(
        &path,
        &Map::from_iter([
            ("version".to_string(), json!(1)),
            ("hooks".to_string(), Value::Object(hooks)),
        ]),
    )
}

fn install_grok(script: &Path) -> anyhow::Result<()> {
    let home = env_path("GROK_HOME").unwrap_or(home_dir()?.join(".grok"));
    let path = home.join("hooks/alera-status.json");
    let hooks = [
        ("SessionStart", None),
        ("UserPromptSubmit", None),
        ("PreToolUse", Some("*")),
        ("PostToolUse", Some("*")),
        ("PostToolUseFailure", Some("*")),
        ("Notification", None),
        ("Stop", None),
        ("StopFailure", None),
        ("StopCancelled", None),
        ("SessionEnd", None),
    ]
    .into_iter()
    .map(|(event, matcher)| {
        (
            event.to_string(),
            json!([managed_hook_definition(
                matcher,
                &managed_command(script, "grok", event)
            )]),
        )
    })
    .collect::<Map<_, _>>();
    write_json_object(
        &path,
        &Map::from_iter([("hooks".to_string(), Value::Object(hooks))]),
    )
}

fn install_devin(script: &Path) -> anyhow::Result<()> {
    let path = devin_user_config_path()?;
    let mut config = read_json_object(&path)?.unwrap_or_default();
    apply_devin_hooks(&mut config, script);
    write_json_object(&path, &config)
}

fn apply_devin_hooks(config: &mut Map<String, Value>, script: &Path) {
    let hooks = object_field(config, "hooks");
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "SessionEnd",
    ] {
        let mut definitions = clean_managed_definitions(hooks.remove(event));
        definitions.push(managed_hook_definition(
            None,
            &devin_managed_command(script, event),
        ));
        hooks.insert(event.to_string(), Value::Array(definitions));
    }
}

fn install_agy(script: &Path) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    for path in agy_hook_config_paths()? {
        if let Err(error) = write_agy_bundle(&path, script) {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(errors.join("; ")))
    }
}

fn write_agy_bundle(path: &Path, script: &Path) -> anyhow::Result<()> {
    let mut config = read_json_object(path)?.unwrap_or_default();
    apply_agy_bundle(&mut config, script);
    write_json_object(path, &config)
}

// Portable agy (Scoop `antigravity_cli_portable`) launches
// `agy.exe --gemini_dir=<persist>\data\gemini` on every start. That flag
// replaces `~/.gemini`, so a home-only hooks file never receives
// PreInvocation or Stop and Alera stays without a running or finished state.
fn agy_hook_config_paths() -> anyhow::Result<Vec<PathBuf>> {
    let mut dirs = vec![home_dir()?.join(".gemini")];
    for dir in discover_agy_gemini_dirs() {
        if !dirs.iter().any(|existing| paths_equivalent(existing, &dir)) {
            dirs.push(dir);
        }
    }
    Ok(dirs
        .into_iter()
        .map(|dir| dir.join("config").join("hooks.json"))
        .collect())
}

fn discover_agy_gemini_dirs() -> Vec<PathBuf> {
    discover_agy_gemini_dirs_from(&agy_launchers_on_path())
}

fn discover_agy_gemini_dirs_from(launchers: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for launcher in launchers {
        walk_agy_launcher(launcher, 0, &mut seen, &mut dirs);
    }
    dirs
}

fn walk_agy_launcher(
    path: &Path,
    depth: usize,
    seen: &mut std::collections::BTreeSet<String>,
    dirs: &mut Vec<PathBuf>,
) {
    if depth > 4 {
        return;
    }
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(normalized_path_key(&key)) {
        return;
    }
    if !is_agy_launcher_script(path) {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() > 256 * 1024 {
        return;
    }
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for dir in gemini_dirs_declared_by_launcher(path, &contents) {
        if dir.is_absolute() && !dirs.iter().any(|existing| paths_equivalent(existing, &dir)) {
            dirs.push(dir);
        }
    }
    for next in launcher_follow_targets(&contents) {
        if next.is_file() {
            walk_agy_launcher(&next, depth + 1, seen, dirs);
        }
    }
}

fn agy_launchers_on_path() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| {
                if extension.starts_with('.') {
                    extension.to_string()
                } else {
                    format!(".{extension}")
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut found = Vec::new();
    for directory in std::env::split_paths(&path) {
        if extensions.is_empty() {
            let candidate = directory.join("agy");
            if candidate.is_file() {
                found.push(candidate);
            }
            continue;
        }
        for extension in &extensions {
            let candidate = directory.join(format!("agy{extension}"));
            if candidate.is_file() {
                found.push(candidate);
            }
        }
    }
    found
}

fn is_agy_launcher_script(path: &Path) -> bool {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
    {
        Some(extension) => matches!(extension.as_str(), "cmd" | "bat" | "ps1" | "sh"),
        None => true,
    }
}

fn gemini_dirs_declared_by_launcher(launcher: &Path, contents: &str) -> Vec<PathBuf> {
    let script_dir = launcher.parent().unwrap_or_else(|| Path::new("."));
    let dp0 = format!("{}{}", script_dir.display(), std::path::MAIN_SEPARATOR);
    let mut variables = BTreeMap::new();
    let mut dirs = Vec::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if let Some((name, value)) = parse_cmd_set(line) {
            variables.insert(name, expand_cmd_vars(&value, &dp0, &variables));
            continue;
        }
        if let Some(value) = gemini_dir_argument(line) {
            let expanded = expand_cmd_vars(&value, &dp0, &variables);
            let path = gemini_dir_path(expanded.trim_matches('"').trim());
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|component| component.as_os_str().to_string_lossy().contains('%'))
            {
                continue;
            }
            dirs.push(path);
        }
    }
    dirs
}

fn parse_cmd_set(line: &str) -> Option<(String, String)> {
    let rest = line
        .strip_prefix("set ")
        .or_else(|| line.strip_prefix("SET "))?;
    let rest = rest.trim().trim_matches('"');
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || name.contains(' ') || name.contains('%') {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

fn gemini_dir_argument(line: &str) -> Option<String> {
    let marker = "--gemini_dir";
    let index = line.find(marker)?;
    let rest = line[index + marker.len()..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    if rest.is_empty() {
        return None;
    }
    if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some(quoted[..end].to_string());
    }
    Some(
        rest.split_whitespace()
            .next()?
            .trim_matches('\'')
            .to_string(),
    )
}

// `%~dp0` is the launcher directory. It is not a `%VAR%` reference: cmd does
// not require a closing percent, which is how the portable wrapper writes
// `set "AGY_PORTABLE_DATA_DIR=%~dp0data\gemini"`.
fn expand_cmd_vars(value: &str, dp0: &str, variables: &BTreeMap<String, String>) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut expanded = String::new();
    let mut index = 0;
    while index < chars.len() {
        if starts_with_ignore_ascii_case(&chars[index..], "%~dp0") {
            expanded.push_str(dp0);
            index += 5;
            continue;
        }
        if chars[index] == '%' {
            if let Some(end) = chars[index + 1..].iter().position(|char| *char == '%') {
                let name: String = chars[index + 1..index + 1 + end].iter().collect();
                if let Some(replacement) = variables.get(&name) {
                    expanded.push_str(replacement);
                } else {
                    expanded.push('%');
                    expanded.push_str(&name);
                    expanded.push('%');
                }
                index += end + 2;
                continue;
            }
        }
        expanded.push(chars[index]);
        index += 1;
    }
    expanded
}

fn gemini_dir_path(value: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(value.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(value.replace('\\', "/"))
    }
}

fn launcher_follow_targets(contents: &str) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line
            .strip_prefix("@rem ")
            .or_else(|| line.strip_prefix("@REM "))
        {
            let path = PathBuf::from(rest.trim());
            if !path.as_os_str().is_empty() {
                targets.push(path);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("@\"") {
            if let Some(end) = rest.find('"') {
                targets.push(PathBuf::from(&rest[..end]));
            }
        }
    }
    targets
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    normalized_path_key(left) == normalized_path_key(right)
}

fn normalized_path_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text.into_owned()
    }
}

fn starts_with_ignore_ascii_case(chars: &[char], token: &str) -> bool {
    let token: Vec<char> = token.chars().collect();
    chars.len() >= token.len()
        && chars
            .iter()
            .zip(token.iter())
            .all(|(char, expected)| char.eq_ignore_ascii_case(expected))
}

// Antigravity keeps each hook set under its own top-level key and uses two
// schemas inside it: lifecycle events take a flat `{ type, command }` handler,
// tool events a matcher wrapping `hooks`. `PreToolUse` is deliberately absent -
// Antigravity requires a permission `decision` from it, which an observational
// hook cannot give without taking over the user's tool policy.
fn apply_agy_bundle(config: &mut Map<String, Value>, script: &Path) {
    let bundle = object_field(config, "alera-status");
    // Installing is an explicit request to enable, so the documented `enabled`
    // opt-out cannot survive it. Every other non-event key is left alone.
    bundle.remove("enabled");
    for event in ["PreInvocation", "PostInvocation", "Stop"] {
        let mut definitions = clean_managed_definitions(bundle.remove(event));
        definitions.push(
            json!({ "type": "command", "command": managed_command(script, "agy", event), "timeout": 10 }),
        );
        bundle.insert(event.to_string(), Value::Array(definitions));
    }
    let mut tool_definitions = clean_managed_definitions(bundle.remove("PostToolUse"));
    tool_definitions.push(
        json!({ "matcher": "*", "hooks": [{ "type": "command", "command": managed_command(script, "agy", "PostToolUse"), "timeout": 10 }] }),
    );
    bundle.insert("PostToolUse".to_string(), Value::Array(tool_definitions));
}

// Non-tool events have nothing to match on. Their schemas expect the key to be
// absent rather than null, so emitting `null` trips agent-side validation.
pub(super) fn managed_hook_definition(matcher: Option<&str>, command: &str) -> Value {
    let mut definition = Map::new();
    if let Some(matcher) = matcher {
        definition.insert("matcher".to_string(), json!(matcher));
    }
    definition.insert(
        "hooks".to_string(),
        json!([{ "type": "command", "command": command }]),
    );
    Value::Object(definition)
}

fn claude_managed_command(script: &Path, event: &str) -> String {
    managed_command(script, "claude", event)
}

fn devin_managed_command(script: &Path, event: &str) -> String {
    managed_command(script, "devin", event)
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowsHookExecutionStrategy {
    NativeCmd,
    GitBashToCmd,
}

#[cfg(windows)]
fn windows_hook_execution_strategy(agent: &str) -> WindowsHookExecutionStrategy {
    match agent {
        // These agents launch shell-form hooks through Git Bash on Windows, so
        // their cmd switches must be protected from MSYS argv conversion.
        "claude" | "devin" => WindowsHookExecutionStrategy::GitBashToCmd,
        // Codex, Grok and Antigravity use native Windows hook runners. Keep all
        // other integrations native by default so POSIX-only bridge syntax is
        // never injected into cmd.exe or PowerShell command parsing.
        _ => WindowsHookExecutionStrategy::NativeCmd,
    }
}

#[cfg(windows)]
fn git_bash_windows_managed_command(script: &Path, agent: &str, event: &str) -> String {
    // Claude Code and Devin execute shell-form hooks through Git Bash on Windows.
    // Keep `call` and the batch path as separate Bash argv. Embedding `call "path"`
    // inside one `/c` argument makes MSYS serialize the inner quotes as literal
    // backslash-quote pairs (`\"`), which cmd.exe does not treat as quoting.
    format!(
        "MSYS2_ARG_CONV_EXCL='*' ALERA_AGENT_TYPE={} ALERA_AGENT_HOOK_EVENT={} cmd.exe /d /s /c call {}",
        sh_quote(agent),
        sh_quote(event),
        sh_quote(&script.display().to_string()),
    )
}

#[cfg(windows)]
fn native_windows_managed_command(script: &Path, agent: &str, event: &str) -> String {
    format!(
        "cmd /d /s /c \"set ALERA_AGENT_TYPE={agent}&& set ALERA_AGENT_HOOK_EVENT={event}&& call \"{}\"\"",
        script.display()
    )
}

pub(super) fn managed_command(script: &Path, agent: &str, event: &str) -> String {
    #[cfg(windows)]
    {
        if agent == "agy" {
            return agy_windows::managed_command(script, event);
        }
        match windows_hook_execution_strategy(agent) {
            WindowsHookExecutionStrategy::NativeCmd => {
                native_windows_managed_command(script, agent, event)
            }
            WindowsHookExecutionStrategy::GitBashToCmd => {
                git_bash_windows_managed_command(script, agent, event)
            }
        }
    }
    #[cfg(not(windows))]
    {
        format!(
            "ALERA_AGENT_TYPE={} ALERA_AGENT_HOOK_EVENT={} /bin/sh {}",
            sh_quote(agent),
            sh_quote(event),
            sh_quote(&path_string(script)),
        )
    }
}

pub(super) fn clean_managed_definitions(value: Option<Value>) -> Vec<Value> {
    value
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|definition| !is_alera_managed_definition(definition))
        .collect()
}

pub(super) fn is_alera_managed_definition(definition: &Value) -> bool {
    let encoded = definition.to_string();
    encoded.contains(MANAGED_MARKER)
        || LEGACY_MANAGED_MARKERS
            .iter()
            .any(|marker| encoded.contains(marker))
}

pub(super) fn object_field<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    if !object.get(key).is_some_and(Value::is_object) {
        object.insert(key.to_string(), Value::Object(Map::new()));
    }
    object
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("object inserted")
}

pub(super) fn read_json_object(path: &Path) -> anyhow::Result<Option<Map<String, Value>>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(serde_json::from_str::<Value>(&contents)?
            .as_object()
            .cloned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn write_json_object(path: &Path, value: &Map<String, Value>) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

pub(super) fn link_if_present(source: &Path, target: &Path) {
    if !source.exists() || target.exists() {
        return;
    }
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(source, target);
    }
    #[cfg(windows)]
    {
        if source.is_dir() {
            let _ = std::os::windows::fs::symlink_dir(source, target);
        } else {
            let _ = std::os::windows::fs::symlink_file(source, target);
        }
    }
}

pub(super) fn home_dir() -> anyhow::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not resolve the user home directory."))
}

fn devin_user_config_path() -> anyhow::Result<PathBuf> {
    #[cfg(windows)]
    {
        let environment = ["DEVIN_PORTABLE_ROOT", "PATH", "PATHEXT"]
            .into_iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| (key.to_string(), value))
            })
            .collect();
        if let Some(path) = devin_paths::portable_config_path(&environment) {
            return Ok(path);
        }
        let root = env_path("APPDATA").unwrap_or(home_dir()?.join("AppData").join("Roaming"));
        Ok(root.join("devin").join("config.json"))
    }
    #[cfg(not(windows))]
    {
        Ok(home_dir()?
            .join(".config")
            .join("devin")
            .join("config.json"))
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "integration_config_claude_tests.rs"]
mod claude_tests;
#[cfg(test)]
#[path = "integration_config_tests.rs"]
mod tests;
