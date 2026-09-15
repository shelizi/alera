use std::collections::BTreeMap;
use std::path::Path;

use alera_core::agent_descriptor::{AgentHookStrategy, AGENT_DESCRIPTORS};
use alera_core::runtime::RuntimeAgentStatusHookSettings;

use super::super::integration_hook_scripts::write_managed_script;
use super::super::integration_plugins::{
    install_amp_plugin, install_opencode2_plugin, install_opencode_plugin, install_pi_plugin,
};
use super::{
    ccs, codex, cursor_overlay, install_agy, install_copilot, install_devin, install_grok,
    prepare_claude, user_hooks,
};

pub fn prepare_enabled_integrations(
    runtime_dir: &Path,
    session_id: Option<&str>,
    settings: &RuntimeAgentStatusHookSettings,
    environment: &mut BTreeMap<String, String>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let script = match write_managed_script() {
        Ok(script) => script,
        Err(error) => {
            warnings.push(error.to_string());
            return warnings;
        }
    };
    for descriptor in AGENT_DESCRIPTORS {
        match descriptor.hook_strategy {
            AgentHookStrategy::RuntimeHome => {
                if !settings.is_enabled(descriptor.id) {
                    if descriptor.id == "claude" {
                        if let Err(error) = super::home_dir().and_then(|home| {
                            user_hooks::cleanup_claude_user_hooks(&home)?;
                            ccs::remove_ccs_claude_hooks(&home, environment)
                        }) {
                            warnings.push(format!("Claude: {error}"));
                        }
                    }
                    continue;
                }
                match descriptor.id {
                    "codex" => match codex::prepare_codex(&script, environment) {
                        Ok(_home) => {
                            // Codex keeps its effective user home. Alera installs
                            // only the managed hook and trust records in place, so
                            // auth, config, plugins, and resume history stay native.
                        }
                        Err(error) => warnings.push(format!("Codex: {error}")),
                    },
                    "claude" => match prepare_claude(runtime_dir, &script, environment) {
                        Ok((_home, ccs_warnings)) => {
                            // Claude must keep using the user's real config home so its
                            // global session/history store remains resumable. Managed
                            // hooks are installed into the effective user settings by
                            // `prepare_claude`, so no config-dir override is required.
                            warnings.extend(
                                ccs_warnings
                                    .into_iter()
                                    .map(|warning| format!("Claude: {warning}")),
                            );
                        }
                        Err(error) => warnings.push(format!("Claude: {error}")),
                    },
                    _ => {}
                }
            }
            AgentHookStrategy::SessionOverlay => {
                // The Cursor plugin is per terminal session, so it can only be
                // built when a session is being launched. `reconcile_agent_integrations` has none.
                if settings.is_enabled(descriptor.id) {
                    if let Some(session_id) = session_id {
                        if let Err(error) = cursor_overlay::prepare_cursor(
                            runtime_dir,
                            session_id,
                            &script,
                            environment,
                        ) {
                            warnings.push(format!("Cursor: {error}"));
                        }
                    }
                }
            }
            AgentHookStrategy::ConfigJson
            | AgentHookStrategy::PluginScript
            | AgentHookStrategy::HerdrSocket
            | AgentHookStrategy::None => {}
        }
    }
    for descriptor in AGENT_DESCRIPTORS {
        if !settings.is_enabled(descriptor.id)
            || descriptor.hook_strategy != AgentHookStrategy::ConfigJson
        {
            continue;
        }
        if let Some(install) = config_json_installer(descriptor.id) {
            if let Err(error) = install(&script) {
                warnings.push(error.to_string());
            }
        }
    }
    for descriptor in AGENT_DESCRIPTORS {
        if !settings.is_enabled(descriptor.id)
            || descriptor.hook_strategy != AgentHookStrategy::PluginScript
        {
            continue;
        }
        if let Some(install) = plugin_installer(descriptor.id) {
            if let Err(error) = install() {
                warnings.push(error.to_string());
            }
        }
    }
    warnings
}

fn config_json_installer(agent_type: &str) -> Option<fn(&Path) -> anyhow::Result<()>> {
    match agent_type {
        "copilot" => Some(install_copilot),
        "agy" => Some(install_agy),
        "grok" => Some(install_grok),
        "devin" => Some(install_devin),
        _ => None,
    }
}

fn plugin_installer(agent_type: &str) -> Option<fn() -> anyhow::Result<()>> {
    match agent_type {
        "opencode" => Some(install_opencode_plugin),
        "opencode2" => Some(install_opencode2_plugin),
        "pi" => Some(install_pi_plugin),
        "amp" => Some(install_amp_plugin),
        _ => None,
    }
}
