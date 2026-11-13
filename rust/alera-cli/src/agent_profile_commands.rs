use std::collections::{HashMap, HashSet};
use std::io::Read;

use alera_core::runtime::{AgentProfile, AgentProfileLaunchMode, AgentProfileRemovalImpact};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::agent_profile_input::{
    draft_for_clone, draft_for_create, draft_for_update, ensure_expected_revision,
    ensure_risk_confirmation, managed_config_for_create, managed_config_for_update,
    parse_revision_overrides,
};
use crate::cli::{
    AgentProfileAction, AgentProfileCommand, AgentProfileReorderArgs, AgentProfileSelectorArgs,
    AgentProfileUpdateArgs,
};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::terminal_host::agent_profile_capabilities::{
    RUNTIME_HOST_AGENT_PROFILE_REMOVAL_CAPABILITY, RUNTIME_HOST_AGENT_PROFILE_REVISIONS_CAPABILITY,
};
use crate::terminal_host::protocol::{
    RUNTIME_HOST_AGENT_PROFILES_CAPABILITY, RUNTIME_HOST_AGENT_PROFILE_ORDERING_CAPABILITY,
    RUNTIME_HOST_MANAGED_AGENT_PROFILES_CAPABILITY,
};

pub(crate) async fn run(command: AgentProfileCommand) -> i32 {
    match run_command(command).await {
        Ok(()) => 0,
        Err(error) => crate::print_error(error),
    }
}

async fn run_command(command: AgentProfileCommand) -> Result<()> {
    let json_output = command.output.json;
    let runtime = crate::runtime_dir(&command.runtime);
    let mut client = if command.no_start {
        RuntimeHostRpcClient::connect_with_required_capability(
            &runtime,
            RUNTIME_HOST_AGENT_PROFILES_CAPABILITY,
        )
        .await?
        .ok_or_else(|| {
            anyhow!("No running compatible Runtime Host; --no-start forbids automatic startup")
        })?
    } else {
        RuntimeHostRpcClient::connect_or_start_with_required_capability(
            &runtime,
            RUNTIME_HOST_AGENT_PROFILES_CAPABILITY,
        )
        .await?
    };
    match command.action {
        AgentProfileAction::List => {
            let (payload, profiles) = list_profiles(&mut client).await?;
            if json_output {
                print_json(&payload)?;
            } else {
                print_profile_list(&profiles);
            }
        }
        AgentProfileAction::Show(selector) => {
            let (_, profiles) = list_profiles(&mut client).await?;
            let profile = select_profile(&profiles, &selector)?;
            if json_output {
                print_json(profile)?;
            } else {
                print_profile_detail(profile)?;
            }
        }
        AgentProfileAction::Create(args) => {
            let managed_config = managed_config_for_create(&args, &mut std::io::stdin().lock())?;
            let draft = draft_for_create(&args, managed_config)?;
            ensure_risk_confirmation(None, &draft, args.confirm_reduced_protections)?;
            if draft.launch_mode == AgentProfileLaunchMode::Managed {
                ensure_capabilities(
                    &mut client,
                    &[RUNTIME_HOST_MANAGED_AGENT_PROFILES_CAPABILITY],
                )
                .await?;
            }
            let saved = client
                .request_value("agentProfile.upsert", &draft.create_payload())
                .await?;
            print_saved(&saved, json_output)?;
        }
        AgentProfileAction::Update(mut args) => {
            apply_stdin_edit_content(&mut args)?;
            let (_, profiles) = list_profiles(&mut client).await?;
            let existing = select_profile(&profiles, &args.target.selector)?;
            let expected_revision =
                ensure_expected_revision(existing, args.target.expected_revision)?;
            let managed_config = if args.edit_managed_selection_stdin {
                Some(read_managed_selection_patch(
                    existing,
                    &mut std::io::stdin().lock(),
                )?)
            } else {
                managed_config_for_update(existing, &args, &mut std::io::stdin().lock())?
            };
            let draft = draft_for_update(existing, &args, managed_config)?;
            ensure_risk_confirmation(Some(existing), &draft, args.confirm_reduced_protections)?;
            let mut capabilities = vec![RUNTIME_HOST_AGENT_PROFILE_REVISIONS_CAPABILITY];
            if draft.launch_mode == AgentProfileLaunchMode::Managed {
                capabilities.push(RUNTIME_HOST_MANAGED_AGENT_PROFILES_CAPABILITY);
            }
            ensure_capabilities(&mut client, &capabilities).await?;
            let saved = client
                .request_value(
                    "agentProfile.upsert",
                    &draft.update_payload(&existing.id, expected_revision),
                )
                .await?;
            print_saved(&saved, json_output)?;
        }
        AgentProfileAction::Clone(args) => {
            if args.expected_revision < 0 {
                bail!("source expected revision must be non-negative");
            }
            let (_, profiles) = list_profiles(&mut client).await?;
            let source = select_profile(&profiles, &args.selector)?;
            ensure_expected_revision(source, Some(args.expected_revision))?;
            let draft = draft_for_clone(source, &args.name)?;
            // A new Managed profile inherits reduced protections; require a
            // fresh explicit confirmation even if the source was configured so.
            ensure_risk_confirmation(None, &draft, args.confirm_reduced_protections)?;
            if draft.launch_mode == AgentProfileLaunchMode::Managed {
                ensure_capabilities(
                    &mut client,
                    &[RUNTIME_HOST_MANAGED_AGENT_PROFILES_CAPABILITY],
                )
                .await?;
            }
            let saved = client
                .request_value("agentProfile.upsert", &draft.create_payload())
                .await?;
            if saved
                .get("id")
                .and_then(Value::as_str)
                .is_none_or(|id| id == source.id)
                || saved.get("revision").and_then(Value::as_i64) != Some(0)
            {
                bail!("Host did not confirm a distinct cloned Agent Profile");
            }
            print_saved(&saved, json_output)?;
        }
        AgentProfileAction::RemovalImpact(args) => {
            let (_, profiles) = list_profiles(&mut client).await?;
            let profile = select_profile(&profiles, &args.selector)?;
            let expected_revision = ensure_expected_revision(profile, args.expected_revision)?;
            ensure_capabilities(
                &mut client,
                &[
                    RUNTIME_HOST_AGENT_PROFILE_REVISIONS_CAPABILITY,
                    RUNTIME_HOST_AGENT_PROFILE_REMOVAL_CAPABILITY,
                ],
            )
            .await?;
            let impact = client
                .request_value(
                    "agentProfile.removalImpact",
                    &json!({"id": profile.id, "expectedRevision": expected_revision}),
                )
                .await?;
            if json_output {
                print_json(&impact)?;
            } else {
                print_removal_impact(&impact, &profile.name)?;
            }
        }
        AgentProfileAction::Remove(args) => {
            let (_, profiles) = list_profiles(&mut client).await?;
            let profile = select_profile(&profiles, &args.target.selector)?;
            let expected_revision =
                ensure_expected_revision(profile, args.target.expected_revision)?;
            ensure_capabilities(
                &mut client,
                &[
                    RUNTIME_HOST_AGENT_PROFILE_REVISIONS_CAPABILITY,
                    RUNTIME_HOST_AGENT_PROFILE_REMOVAL_CAPABILITY,
                ],
            )
            .await?;
            let impact_value = client
                .request_value(
                    "agentProfile.removalImpact",
                    &json!({"id": profile.id, "expectedRevision": expected_revision}),
                )
                .await?;
            let impact: AgentProfileRemovalImpact = serde_json::from_value(impact_value)
                .context("runtime host returned an invalid agent profile removal impact")?;
            if impact.has_blocking_references() {
                bail!(
                    "agent profile removal is blocked by {} reference(s); run removal-impact for details",
                    impact.automation_ids.len()
                        + impact.execution_policy_run_ids.len()
                        + impact.tabs.len()
                );
            }
            let removed = client
                .request_value(
                    "agentProfile.remove",
                    &json!({
                        "id": profile.id,
                        "expectedRevision": expected_revision,
                        "confirmed": args.confirm,
                    }),
                )
                .await?;
            if json_output {
                print_json(&removed)?;
            } else if removed.get("removed").and_then(Value::as_bool) == Some(true) {
                println!("agent profile removed: {} ({})", profile.name, profile.id);
            } else {
                println!("agent profile was already absent: {}", profile.id);
            }
        }
        AgentProfileAction::Launch(args) => {
            crate::agent_profile_launch::run(&command.runtime, &mut client, args, json_output)
                .await?;
        }
        AgentProfileAction::Reorder(args) => {
            let (_, profiles) = list_profiles(&mut client).await?;
            let expected_revisions = reorder_revisions(&profiles, &args)?;
            ensure_capabilities(
                &mut client,
                &[
                    RUNTIME_HOST_AGENT_PROFILE_REVISIONS_CAPABILITY,
                    RUNTIME_HOST_AGENT_PROFILE_ORDERING_CAPABILITY,
                ],
            )
            .await?;
            let payload = client
                .request_value(
                    "agentProfile.reorder",
                    &json!({"ids": args.ids, "expectedRevisions": expected_revisions}),
                )
                .await?;
            if json_output {
                print_json(&payload)?;
            } else {
                let reordered = profiles_from_payload(&payload)?;
                print_profile_list(&reordered);
            }
        }
    }
    Ok(())
}

pub(crate) async fn list_profiles(
    client: &mut RuntimeHostRpcClient,
) -> Result<(Value, Vec<AgentProfile>)> {
    let payload = client
        .request_value("agentProfile.list", &json!({}))
        .await?;
    let profiles = profiles_from_payload(&payload)?;
    Ok((payload, profiles))
}

fn profiles_from_payload(payload: &Value) -> Result<Vec<AgentProfile>> {
    let items = payload
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("runtime agent profile payload must carry an items array"))?;
    items
        .iter()
        .cloned()
        .map(|value| {
            serde_json::from_value(value).context("runtime host returned an invalid agent profile")
        })
        .collect()
}

pub(crate) fn select_profile<'a>(
    profiles: &'a [AgentProfile],
    selector: &AgentProfileSelectorArgs,
) -> Result<&'a AgentProfile> {
    if let Some(id) = selector.profile_id.as_deref() {
        let id = id.trim();
        if id.is_empty() {
            bail!("profile id cannot be empty");
        }
        return profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| anyhow!("agent profile not found: {id}"));
    }
    let name = selector
        .profile_name_or_alias()
        .ok_or_else(|| anyhow!("profile name cannot be empty"))?;
    profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("agent profile not found: {name}"))
}

pub(crate) async fn ensure_capabilities(
    client: &mut RuntimeHostRpcClient,
    required: &[&str],
) -> Result<()> {
    if required.is_empty() {
        return Ok(());
    }
    let status = client.request_value("status.get", &json!({})).await?;
    let capabilities = status
        .get("runtimeCapabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("runtime host status did not include capabilities"))?;
    for required in required {
        if !capabilities
            .iter()
            .any(|capability| capability.as_str() == Some(required))
        {
            bail!(
                "A live Alera runtime host does not support {required}. Restart Alera and retry."
            );
        }
    }
    Ok(())
}

const MAX_MANAGED_SELECTION_BYTES: u64 = 4096;
const MAX_MANAGED_CHOICE_BYTES: usize = 256;

/// Narrow Host-owned Managed edit. Never reconstruct the whole Managed config
/// from Native's display projection, and never place model/persona in argv.
/// The original config (including unrecognized and risk-related options) is
/// preserved, then passed through existing risk confirmation and Host CAS.
fn read_managed_selection_patch(existing: &AgentProfile, stdin: &mut impl Read) -> Result<Value> {
    if existing.launch_mode != AgentProfileLaunchMode::Managed {
        bail!("model/persona editing requires an existing Managed Profile");
    }
    let descriptor = alera_core::agent_descriptor::agent_descriptor(&existing.agent_type)
        .ok_or_else(|| anyhow!("Unknown Agent adapter; cannot edit Managed selection"))?;
    let mut raw = String::new();
    stdin
        .by_ref()
        .take(MAX_MANAGED_SELECTION_BYTES + 1)
        .read_to_string(&mut raw)
        .context("failed to read Managed model/persona patch from stdin")?;
    if raw.len() as u64 > MAX_MANAGED_SELECTION_BYTES {
        bail!("Managed model/persona patch exceeds size limit");
    }
    let value: Value =
        serde_json::from_str(&raw).context("Managed model/persona patch must be valid JSON")?;
    let edits = value
        .as_object()
        .ok_or_else(|| anyhow!("Managed model/persona patch must be an object"))?;
    if edits.is_empty()
        || edits
            .keys()
            .any(|key| !matches!(key.as_str(), "model" | "persona"))
    {
        bail!("Managed model/persona patch requires only model or persona fields");
    }
    let mut config = existing
        .managed_config
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| anyhow!("Existing Managed config is unavailable"))?;
    for (key, value) in edits {
        if key == "model"
            && descriptor.model_override
                == alera_core::agent_descriptor::AgentModelOverride::Unsupported
        {
            bail!("This Agent adapter does not support model selection");
        }
        if key == "persona" && !descriptor.supports_persona {
            bail!("This Agent adapter does not support persona selection");
        }
        // Classic's Persona selector is stored in the Host launch spec's
        // `agent` field, never an unrecognized `persona` config key.
        let managed_key = if key == "persona" {
            "agent"
        } else {
            key.as_str()
        };
        match value {
            Value::Null => {
                config.remove(managed_key);
            }
            Value::String(text) => {
                let normalized = text.trim();
                if normalized.is_empty()
                    || normalized.len() > MAX_MANAGED_CHOICE_BYTES
                    || normalized.chars().any(char::is_control)
                {
                    bail!("Managed model/persona selection must be a nonempty safe string");
                }
                config.insert(managed_key.to_owned(), Value::String(normalized.to_owned()));
            }
            _ => bail!("Managed model/persona fields must be strings or null to clear"),
        }
    }
    Ok(Value::Object(config))
}

const MAX_EDIT_CONTENT_STDIN_BYTES: u64 = 32 * 1024;
const MAX_EDIT_COMMAND_BYTES: usize = 4 * 1024;
const MAX_EDIT_PROMPT_BYTES: usize = 8 * 1024;

struct AgentProfileEditContent {
    command: Option<String>,
    custom_prompt: Option<String>,
    clear_custom_prompt: bool,
}

/// `--edit-content-stdin` carries Command/Custom Prompt edits as bounded JSON
/// so callers never place user text in process argv or logs. Parsing is
/// all-or-nothing: a malformed body leaves every profile field untouched.
fn apply_stdin_edit_content(args: &mut AgentProfileUpdateArgs) -> Result<()> {
    if !args.edit_content_stdin {
        return Ok(());
    }
    let edit = read_edit_content(&mut std::io::stdin().lock())?;
    apply_edit_content(args, edit);
    Ok(())
}

fn read_edit_content(stdin: &mut impl Read) -> Result<AgentProfileEditContent> {
    let mut raw = String::new();
    stdin
        .by_ref()
        .take(MAX_EDIT_CONTENT_STDIN_BYTES + 1)
        .read_to_string(&mut raw)
        .context("failed to read agent profile edit content from standard input")?;
    if raw.len() as u64 > MAX_EDIT_CONTENT_STDIN_BYTES {
        bail!(
            "agent profile edit content exceeds {} bytes",
            MAX_EDIT_CONTENT_STDIN_BYTES
        );
    }
    parse_edit_content(&raw)
}

fn apply_edit_content(args: &mut AgentProfileUpdateArgs, edit: AgentProfileEditContent) {
    if let Some(command) = edit.command {
        args.command = Some(command);
    }
    if let Some(prompt) = edit.custom_prompt {
        args.custom_prompt = Some(prompt);
    }
    if edit.clear_custom_prompt {
        args.clear_custom_prompt = true;
    }
}

fn parse_edit_content(raw: &str) -> Result<AgentProfileEditContent> {
    let value: Value =
        serde_json::from_str(raw).context("agent profile edit content must be valid JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("agent profile edit content must be a JSON object"))?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "command" | "customPrompt" | "clearCustomPrompt"
        ) {
            bail!("agent profile edit content has unsupported field: {key}");
        }
    }
    let command = edit_text_field(object, "command", MAX_EDIT_COMMAND_BYTES)?;
    let custom_prompt = match object.get("customPrompt") {
        None => None,
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                bail!(
                    "agent profile edit content customPrompt must be nonempty; use clearCustomPrompt to clear it"
                );
            }
            if trimmed.len() > MAX_EDIT_PROMPT_BYTES {
                bail!(
                    "agent profile edit content customPrompt exceeds {MAX_EDIT_PROMPT_BYTES} bytes"
                );
            }
            if trimmed.contains('\0') {
                bail!("agent profile edit content customPrompt must not contain NUL");
            }
            Some(trimmed.to_string())
        }
        Some(_) => bail!("agent profile edit content customPrompt must be a string"),
    };
    let clear_custom_prompt = match object.get("clearCustomPrompt") {
        None => false,
        Some(Value::Bool(true)) => true,
        Some(_) => {
            bail!("agent profile edit content clearCustomPrompt must be true when present")
        }
    };
    if custom_prompt.is_some() && clear_custom_prompt {
        bail!("agent profile edit content cannot replace and clear customPrompt together");
    }
    if command.is_none() && custom_prompt.is_none() && !clear_custom_prompt {
        bail!("agent profile edit content must set command, customPrompt, or clearCustomPrompt");
    }
    Ok(AgentProfileEditContent {
        command,
        custom_prompt,
        clear_custom_prompt,
    })
}

fn edit_text_field(
    object: &Map<String, Value>,
    key: &str,
    max_bytes: usize,
) -> Result<Option<String>> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    let Value::String(text) = value else {
        bail!("agent profile edit content {key} must be a string");
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("agent profile edit content {key} must be nonempty");
    }
    if trimmed.len() > max_bytes {
        bail!("agent profile edit content {key} exceeds {max_bytes} bytes");
    }
    if trimmed.contains('\0') {
        bail!("agent profile edit content {key} must not contain NUL");
    }
    Ok(Some(trimmed.to_string()))
}

fn reorder_revisions(
    profiles: &[AgentProfile],
    args: &AgentProfileReorderArgs,
) -> Result<HashMap<String, i64>> {
    let requested = args
        .ids
        .iter()
        .map(|id| id.trim().to_string())
        .collect::<Vec<_>>();
    if requested.iter().any(String::is_empty) {
        bail!("profile ids in the requested order cannot be empty");
    }
    let unique = requested.iter().collect::<HashSet<_>>();
    if unique.len() != requested.len() {
        bail!("agent profile order must contain each profile exactly once");
    }
    let current = profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<HashSet<_>>();
    let requested_set = requested.iter().map(String::as_str).collect::<HashSet<_>>();
    if requested_set != current {
        bail!("agent profile order must contain every current profile exactly once");
    }
    let mut revisions = profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile.revision))
        .collect::<HashMap<_, _>>();
    for (id, revision) in parse_revision_overrides(&args.expected_revisions)? {
        if !revisions.contains_key(&id) {
            bail!("expected revision references an unknown profile: {id}");
        }
        revisions.insert(id, revision);
    }
    Ok(revisions)
}

fn print_saved(value: &Value, json_output: bool) -> Result<()> {
    if json_output {
        return print_json(value);
    }
    let profile: AgentProfile = serde_json::from_value(value.clone())
        .context("runtime host returned an invalid saved agent profile")?;
    println!(
        "agent profile saved: {} ({}, revision {})",
        profile.name, profile.id, profile.revision
    );
    Ok(())
}

fn print_profile_list(profiles: &[AgentProfile]) {
    if profiles.is_empty() {
        println!("no agent profiles declared");
        return;
    }
    println!("{} agent profile(s)", profiles.len());
    for profile in profiles {
        let quota = profile
            .quota_group
            .as_deref()
            .map(|group| format!(" [{group}]"))
            .unwrap_or_default();
        println!(
            "  {} | {} | {} | {} | revision {}{}",
            profile.id,
            profile.name,
            profile.agent_type,
            profile.launch_mode.as_str(),
            profile.revision,
            quota
        );
    }
}

fn print_profile_detail(profile: &AgentProfile) -> Result<()> {
    print_json(profile)
}

fn print_removal_impact(value: &Value, profile_name: &str) -> Result<()> {
    let impact: AgentProfileRemovalImpact = serde_json::from_value(value.clone())
        .context("runtime host returned an invalid agent profile removal impact")?;
    let reference_count = value
        .get("referenceCount")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| impact.reference_count() as u64);
    let blocking_count = value
        .get("blockingReferenceCount")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            (impact.automation_ids.len()
                + impact.execution_policy_run_ids.len()
                + impact.tabs.len()) as u64
        });
    println!("agent profile removal impact: {profile_name}");
    println!("  references: {reference_count}");
    println!("  blocking references: {blocking_count}");
    println!("  default profile: {}", impact.is_default);
    if !impact.automation_ids.is_empty() {
        println!("  automations: {}", impact.automation_ids.join(", "));
    }
    if !impact.execution_policy_run_ids.is_empty() {
        println!(
            "  execution policies: {}",
            impact.execution_policy_run_ids.join(", ")
        );
    }
    if !impact.tabs.is_empty() {
        println!("  recoverable tabs: {}", impact.tabs.len());
    }
    Ok(())
}

pub(crate) fn print_json(value: &impl Serialize) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("failed to render JSON output")?
    );
    Ok(())
}

#[cfg(test)]
#[path = "agent_profile_commands_tests.rs"]
mod tests;
