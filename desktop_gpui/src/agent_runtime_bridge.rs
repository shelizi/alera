//! Narrow Agent Profile CLI bridge. The existing Alera CLI owns all Host
//! authentication, Agent launch semantics, MCP injection and PTY lifetimes.
//! No generic process runner is exposed to GPUI views.
use crate::agent_profiles::{
    parse_launched_agent, parse_profile_catalog, parse_profile_content_fields,
    parse_profile_managed_selection_fields, parse_profile_removal_impact,
    parse_profile_routing_fields, safe_runtime_id, validate_managed_selection_text,
    validate_profile_name, validate_profile_routing_fields, AgentProfileContentFields,
    AgentProfileManagedSelectionFields, AgentProfileRemovalImpact, AgentProfileRoutingFields,
    AgentProfileRow, LaunchedAgentTab,
};
use crate::runtime_bridge::RuntimeCliBridge;
use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_EDIT_COMMAND_BYTES: usize = 4 * 1024;
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

enum AllowedAgentRequest<'a> {
    List,
    Show {
        id: &'a str,
    },
    UpdateRouting {
        id: &'a str,
        expected_revision: i64,
        description: &'a str,
        quota_group: &'a str,
    },
    /// Command/Custom Prompt edits travel as stdin JSON (`content`); argv
    /// carries only the --edit-content-stdin opt-in flag.
    UpdateCommandPrompt {
        id: &'a str,
        expected_revision: i64,
        content: &'a str,
    },
    /// Only a bounded model/persona patch goes over stdin. The CLI merges
    /// into Host's full Managed config, preserving every other option.
    UpdateManagedSelection {
        id: &'a str,
        expected_revision: i64,
        content: &'a str,
    },
    CreateCommand {
        name: &'a str,
        agent_type: &'a str,
        command: &'a str,
    },
    Clone {
        id: &'a str,
        expected_revision: i64,
        name: &'a str,
    },
    Rename {
        id: &'a str,
        expected_revision: i64,
        name: &'a str,
    },
    Reorder {
        profiles: &'a [AgentProfileRow],
    },
    RemovalImpact {
        id: &'a str,
        expected_revision: i64,
    },
    RemoveConfirmed {
        id: &'a str,
        expected_revision: i64,
    },
    Launch {
        profile_id: &'a str,
        workspace_id: &'a str,
        prompt: &'a str,
    },
}

fn args_for(runtime: &Path, request: &AllowedAgentRequest<'_>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("agent-profile"),
        OsString::from("--no-start"),
        OsString::from("--json"),
        OsString::from("--runtime-dir"),
        runtime.as_os_str().to_owned(),
    ];
    match request {
        AllowedAgentRequest::List => args.push(OsString::from("list")),
        AllowedAgentRequest::Show { id } => {
            args.push(OsString::from("show"));
            args.push(OsString::from(format!("--profile-id={id}")));
        }
        AllowedAgentRequest::UpdateRouting {
            id,
            expected_revision,
            description,
            quota_group,
        } => {
            args.push(OsString::from("update"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            if description.is_empty() {
                args.push(OsString::from("--clear-description"));
            } else {
                args.push(OsString::from(format!("--description={description}")));
            }
            if quota_group.is_empty() {
                args.push(OsString::from("--clear-quota-group"));
            } else {
                args.push(OsString::from(format!("--quota-group={quota_group}")));
            }
        }
        AllowedAgentRequest::UpdateCommandPrompt {
            id,
            expected_revision,
            ..
        } => {
            args.push(OsString::from("update"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from("--edit-content-stdin"));
        }
        AllowedAgentRequest::UpdateManagedSelection {
            id,
            expected_revision,
            ..
        } => {
            args.push(OsString::from("update"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from("--edit-managed-selection-stdin"));
        }
        AllowedAgentRequest::CreateCommand {
            name,
            agent_type,
            command,
        } => {
            args.push(OsString::from("create"));
            args.push(OsString::from(format!("--name={name}")));
            args.push(OsString::from(format!("--agent-type={agent_type}")));
            args.push(OsString::from("--launch-mode=command"));
            args.push(OsString::from(format!("--command={command}")));
        }
        AllowedAgentRequest::Clone {
            id,
            expected_revision,
            name,
        } => {
            args.push(OsString::from("clone"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from(format!("--name={name}")));
        }
        AllowedAgentRequest::Rename {
            id,
            expected_revision,
            name,
        } => {
            args.push(OsString::from("update"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from(format!("--name={name}")));
        }
        AllowedAgentRequest::Reorder { profiles } => {
            args.push(OsString::from("reorder"));
            for profile in *profiles {
                args.push(OsString::from(format!("--id={}", profile.id)));
            }
            for profile in *profiles {
                args.push(OsString::from(format!(
                    "--expected-revision={}={}",
                    profile.id,
                    profile.revision.unwrap_or(-1)
                )));
            }
        }
        AllowedAgentRequest::RemovalImpact {
            id,
            expected_revision,
        } => {
            args.push(OsString::from("removal-impact"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
        }
        AllowedAgentRequest::RemoveConfirmed {
            id,
            expected_revision,
        } => {
            args.push(OsString::from("remove"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from("--confirm"));
        }
        AllowedAgentRequest::Launch {
            profile_id,
            workspace_id,
            ..
        } => {
            args.push(OsString::from("launch"));
            args.push(OsString::from(format!("--profile-id={profile_id}")));
            args.push(OsString::from(format!("--workspace={workspace_id}")));
            args.push(OsString::from("--prompt-stdin"));
        }
    }
    args
}

fn parse_saved_profile(value: Value) -> io::Result<AgentProfileRow> {
    let mut profiles = parse_profile_catalog(&serde_json::json!({"items":[value]}))?;
    profiles.pop().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Host returned no saved Agent Profile",
        )
    })
}

fn wait_bounded(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(code)) => return Ok(code),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Agent CLI timed out; its Host request may already have been applied",
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

fn validate_edit_command(raw: &str) -> io::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_EDIT_COMMAND_BYTES || trimmed.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command must be nonempty, NUL-free, and at most 4 KiB",
        ));
    }
    Ok(trimmed.to_owned())
}

fn validate_edit_prompt(raw: &str) -> io::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_PROMPT_BYTES || trimmed.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent Custom Prompt must be nonempty, NUL-free, and at most 8 KiB",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Build the exact stdin JSON the CLI's --edit-content-stdin parser accepts.
/// Only validated, already-trimmed values are serialized.
fn command_prompt_edit_json(
    command: Option<String>,
    custom_prompt: Option<String>,
    clear_custom_prompt: bool,
) -> String {
    let mut edit = serde_json::Map::new();
    if let Some(command) = command {
        edit.insert("command".to_string(), Value::String(command));
    }
    if let Some(prompt) = custom_prompt {
        edit.insert("customPrompt".to_string(), Value::String(prompt));
    }
    if clear_custom_prompt {
        edit.insert("clearCustomPrompt".to_string(), Value::Bool(true));
    }
    Value::Object(edit).to_string()
}

impl RuntimeCliBridge {
    pub fn list_agent_profiles(&self) -> io::Result<Vec<AgentProfileRow>> {
        let response = self.invoke_existing_host_agent(AllowedAgentRequest::List)?;
        parse_profile_catalog(&response)
    }

    /// Create only a normal command-mode profile with the canonical default
    /// executable. Advanced managed settings remain Host/Classic owned.
    pub fn create_command_agent_profile(
        &self,
        raw_name: &str,
        agent_type: &str,
    ) -> io::Result<AgentProfileRow> {
        let name = validate_profile_name(raw_name)?;
        let descriptor = alera_core::agent_descriptor::agent_descriptor(agent_type)
            .filter(|d| d.id == agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown Agent adapter"))?;
        let saved = self.invoke_existing_host_agent(AllowedAgentRequest::CreateCommand {
            name: &name,
            agent_type,
            command: descriptor.default_command,
        })?;
        let row = parse_saved_profile(saved)?;
        if row.name != name || row.agent_type != agent_type || row.managed || row.revision.is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm created Agent Profile",
            ));
        }
        Ok(row)
    }

    /// Clone by Host-owned identity, never by reconstructing a profile from
    /// Native's intentionally secret-free display projection.
    pub fn clone_agent_profile(
        &self,
        source: &AgentProfileRow,
        raw_name: &str,
    ) -> io::Result<AgentProfileRow> {
        let name = validate_profile_name(raw_name)?;
        let revision = source.revision.filter(|n| *n >= 0).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Missing Host Profile revision")
        })?;
        if !safe_runtime_id(&source.id, 128) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid source Profile ID",
            ));
        }
        let saved = self.invoke_existing_host_agent(AllowedAgentRequest::Clone {
            id: &source.id,
            expected_revision: revision,
            name: &name,
        })?;
        let row = parse_saved_profile(saved)?;
        if row.id == source.id
            || row.name != name
            || row.agent_type != source.agent_type
            || row.managed != source.managed
            || row.revision != Some(0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm a distinct matching Agent Profile copy",
            ));
        }
        Ok(row)
    }

    /// Display-only Host routing metadata: never store the full Show response
    /// in a Native view or settings file.
    pub fn load_agent_profile_routing(
        &self,
        selected: &AgentProfileRow,
    ) -> io::Result<AgentProfileRoutingFields> {
        if !safe_runtime_id(&selected.id, 128)
            || selected.revision.is_none_or(|revision| revision < 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A valid Host Profile revision is required to load Routing",
            ));
        }
        let value =
            self.invoke_existing_host_agent(AllowedAgentRequest::Show { id: &selected.id })?;
        let fields = parse_profile_routing_fields(&value)?;
        if fields.id != selected.id || Some(fields.revision) != selected.revision {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host Profile changed; refresh before editing Routing",
            ));
        }
        Ok(fields)
    }

    /// User-initiated, transient Host content read. This projection must
    /// remain in the active edit UI only and is never copied to shared settings.
    pub fn load_agent_profile_content(
        &self,
        selected: &AgentProfileRow,
    ) -> io::Result<AgentProfileContentFields> {
        if !safe_runtime_id(&selected.id, 128)
            || selected.revision.is_none_or(|revision| revision < 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "An observed Host revision is required to edit Profile content",
            ));
        }
        let response =
            self.invoke_existing_host_agent(AllowedAgentRequest::Show { id: &selected.id })?;
        let content = parse_profile_content_fields(&response)?;
        if content.id != selected.id
            || Some(content.revision) != selected.revision
            || content.managed != selected.managed
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host Profile changed; refresh before editing Command/Prompt",
            ));
        }
        Ok(content)
    }

    /// Read only model/persona from a Host Show response, discard all other
    /// Managed options without logging or storing them in Native preferences.
    pub fn load_agent_profile_managed_selection(
        &self,
        selected: &AgentProfileRow,
    ) -> io::Result<AgentProfileManagedSelectionFields> {
        if !selected.managed
            || !safe_runtime_id(&selected.id, 128)
            || selected.revision.is_none_or(|revision| revision < 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "A selected Managed Profile and observed revision are required",
            ));
        }
        let response =
            self.invoke_existing_host_agent(AllowedAgentRequest::Show { id: &selected.id })?;
        let fields = parse_profile_managed_selection_fields(&response)?;
        if fields.id != selected.id || Some(fields.revision) != selected.revision {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host Managed Profile changed; refresh before editing",
            ));
        }
        Ok(fields)
    }

    /// Update the model/persona allowlist only. The CLI reads a short stdin
    /// JSON patch, merges it with the live Host Managed options, checks revision
    /// and applies its existing reduced-protections confirmation gate.
    pub fn update_agent_profile_managed_selection(
        &self,
        selected: &AgentProfileRow,
        model: Option<Option<&str>>,
        persona: Option<Option<&str>>,
    ) -> io::Result<AgentProfileRow> {
        let revision = selected
            .revision
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Managed edit needs a current Host revision",
                )
            })?;
        if !selected.managed || !safe_runtime_id(&selected.id, 128) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Managed edit needs a valid Managed Host Profile",
            ));
        }
        let descriptor = alera_core::agent_descriptor::agent_descriptor(&selected.agent_type)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Unknown Managed Agent adapter")
            })?;
        if model.is_some()
            && descriptor.model_override
                == alera_core::agent_descriptor::AgentModelOverride::Unsupported
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "This Agent adapter does not support Model selection",
            ));
        }
        if persona.is_some() && !descriptor.supports_persona {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "This Agent adapter does not support Persona selection",
            ));
        }
        let mut edit = serde_json::Map::new();
        let append = |edit: &mut serde_json::Map<String, Value>,
                      key: &str,
                      value: Option<Option<&str>>|
         -> io::Result<()> {
            if let Some(value) = value {
                edit.insert(
                    key.to_owned(),
                    match value {
                        Some(text) => Value::String(validate_managed_selection_text(text)?),
                        None => Value::Null,
                    },
                );
            }
            Ok(())
        };
        append(&mut edit, "model", model)?;
        append(&mut edit, "persona", persona)?;
        if edit.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Select a Model/Persona change before saving",
            ));
        }
        let content = Value::Object(edit).to_string();
        let response =
            self.invoke_existing_host_agent(AllowedAgentRequest::UpdateManagedSelection {
                id: &selected.id,
                expected_revision: revision,
                content: &content,
            })?;
        let saved = parse_saved_profile(response)?;
        if saved.id != selected.id
            || saved.revision.is_none_or(|updated| updated <= revision)
            || saved.agent_type != selected.agent_type
            || !saved.managed
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm a revision-guarded Managed edit",
            ));
        }
        let verified = self.load_agent_profile_managed_selection(&saved)?;
        if model.is_some_and(|value| match value {
            Some(text) => verified.model != text.trim(),
            None => !verified.model.is_empty(),
        }) || persona.is_some_and(|value| match value {
            Some(text) => verified.persona != text.trim(),
            None => !verified.persona.is_empty(),
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host Managed selection readback differs from saved edit",
            ));
        }
        Ok(saved)
    }

    /// Patch only routing fields via CLI update; the CLI's Host draft retains
    /// Command, Custom Prompt and the full Managed configuration unchanged.
    pub fn save_agent_profile_routing(
        &self,
        selected: &AgentProfileRow,
        raw_description: &str,
        raw_group: &str,
    ) -> io::Result<AgentProfileRow> {
        let revision = selected
            .revision
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Missing observed Host Profile revision",
                )
            })?;
        if !safe_runtime_id(&selected.id, 128) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Host Profile ID",
            ));
        }
        let (description, quota_group) =
            validate_profile_routing_fields(raw_description, raw_group)?;
        let value = self.invoke_existing_host_agent(AllowedAgentRequest::UpdateRouting {
            id: &selected.id,
            expected_revision: revision,
            description: &description,
            quota_group: &quota_group,
        })?;
        let fields = parse_profile_routing_fields(&value)?;
        let row = parse_saved_profile(value)?;
        if fields.id != selected.id
            || row.id != selected.id
            || fields.description != description
            || fields.quota_group != quota_group
            || row.revision.is_none_or(|saved| saved <= revision)
            || row.agent_type != selected.agent_type
            || row.managed != selected.managed
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm a revision-guarded Routing update",
            ));
        }
        Ok(row)
    }

    /// Update only the Host-owned Command line and/or Custom Prompt. Both
    /// travel to the CLI as bounded stdin JSON, never argv; the transient
    /// Host reply is re-projected to the secret-free row and must advance
    /// the observed revision. Untouched fields stay Host-owned.
    pub fn update_agent_profile_command_prompt(
        &self,
        selected: &AgentProfileRow,
        command: Option<&str>,
        custom_prompt: Option<&str>,
        clear_custom_prompt: bool,
    ) -> io::Result<AgentProfileRow> {
        let revision = selected
            .revision
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Missing observed Host Profile revision",
                )
            })?;
        if !safe_runtime_id(&selected.id, 128) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Host Profile ID",
            ));
        }
        let command = command.map(validate_edit_command).transpose()?;
        let custom_prompt = custom_prompt.map(validate_edit_prompt).transpose()?;
        if custom_prompt.is_some() && clear_custom_prompt {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Custom Prompt cannot be replaced and cleared together",
            ));
        }
        if command.is_none() && custom_prompt.is_none() && !clear_custom_prompt {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Command/Prompt update needs a command, prompt, or explicit clear",
            ));
        }
        if command.is_some() && selected.managed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Command edits require a Command-mode Agent Profile",
            ));
        }
        let expected_command = command.clone();
        let expected_prompt = custom_prompt.clone();
        let content = command_prompt_edit_json(command, custom_prompt, clear_custom_prompt);
        let value = self.invoke_existing_host_agent(AllowedAgentRequest::UpdateCommandPrompt {
            id: &selected.id,
            expected_revision: revision,
            content: content.as_str(),
        })?;
        let row = parse_saved_profile(value)?;
        if row.id != selected.id
            || row.revision.is_none_or(|saved| saved <= revision)
            || row.agent_type != selected.agent_type
            || row.managed != selected.managed
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm a revision-guarded Command/Prompt update",
            ));
        }
        let verified = self.load_agent_profile_content(&row)?;
        if expected_command
            .as_ref()
            .is_some_and(|command| verified.command != *command)
            || expected_prompt
                .as_ref()
                .is_some_and(|prompt| verified.custom_prompt != *prompt)
            || (clear_custom_prompt && !verified.custom_prompt.is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host Command/Prompt readback differs from saved edit",
            ));
        }
        Ok(row)
    }

    /// Rename only: leave Host-managed command/config/prompt/credentials
    /// untouched. Refuse overwriting a revision the user never observed.
    pub fn rename_agent_profile(
        &self,
        profile_id: &str,
        expected_revision: i64,
        raw_name: &str,
    ) -> io::Result<AgentProfileRow> {
        if !safe_runtime_id(profile_id, 128) || expected_revision < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile revision or ID",
            ));
        }
        let name = validate_profile_name(raw_name)?;
        let saved = self.invoke_existing_host_agent(AllowedAgentRequest::Rename {
            id: profile_id,
            expected_revision,
            name: &name,
        })?;
        let row = parse_saved_profile(saved)?;
        if row.id != profile_id
            || row.name != name
            || row
                .revision
                .is_none_or(|revision| revision <= expected_revision)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm revision-guarded Agent Profile rename",
            ));
        }
        Ok(row)
    }

    /// Change exactly one adjacent position using the complete Host revision
    /// snapshot. Never reorder silently when another view changed the catalog.
    pub fn move_agent_profile(
        &self,
        observed: &[AgentProfileRow],
        profile_id: &str,
        toward_start: bool,
    ) -> io::Result<Vec<AgentProfileRow>> {
        if observed.len() < 2 || observed.len() > 96 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Profile reorder needs at least two Host Profiles",
            ));
        }
        for (index, profile) in observed.iter().enumerate() {
            if !safe_runtime_id(&profile.id, 128)
                || profile.revision.is_none_or(|revision| revision < 0)
                || observed[..index]
                    .iter()
                    .any(|previous| previous.id == profile.id)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Profile reorder requires unique IDs and all Host revisions",
                ));
            }
        }
        let Some(from) = observed.iter().position(|profile| profile.id == profile_id) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selected Agent Profile is no longer listed",
            ));
        };
        let target = if toward_start {
            from.checked_sub(1)
        } else {
            (from + 1 < observed.len()).then_some(from + 1)
        };
        let Some(to) = target else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent Profile is already at this end",
            ));
        };
        let mut ordered = observed.to_vec();
        ordered.swap(from, to);
        let response =
            self.invoke_existing_host_agent(AllowedAgentRequest::Reorder { profiles: &ordered })?;
        let readback = parse_profile_catalog(&response)?;
        if readback.len() != ordered.len()
            || readback
                .iter()
                .zip(&ordered)
                .any(|(actual, requested)| actual.id != requested.id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm requested Agent Profile order",
            ));
        }
        Ok(readback)
    }

    /// Preview every Host reference before presenting an explicit deletion confirmation.
    pub fn preview_agent_profile_removal(
        &self,
        profile_id: &str,
        expected_revision: i64,
    ) -> io::Result<AgentProfileRemovalImpact> {
        if !safe_runtime_id(profile_id, 128) || expected_revision < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile removal ID or revision",
            ));
        }
        let result = self.invoke_existing_host_agent(AllowedAgentRequest::RemovalImpact {
            id: profile_id,
            expected_revision,
        })?;
        parse_profile_removal_impact(&result, profile_id, expected_revision)
    }

    /// Only after an explicit UI confirmation: refetch the exact impact before
    /// issuing Host's revision-guarded --confirm. Host remains authoritative.
    pub fn remove_agent_profile(&self, observed: &AgentProfileRemovalImpact) -> io::Result<()> {
        if observed.has_blocking_references() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Profile is still used by active automation, policies or tabs",
            ));
        }
        let fresh = self.preview_agent_profile_removal(&observed.profile_id, observed.revision)?;
        if fresh != *observed {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host Profile references changed since deletion preview; preview again",
            ));
        }
        let reply = self.invoke_existing_host_agent(AllowedAgentRequest::RemoveConfirmed {
            id: &observed.profile_id,
            expected_revision: observed.revision,
        })?;
        if reply.get("removed").and_then(Value::as_bool) != Some(true) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm Agent Profile removal",
            ));
        }
        if self
            .list_agent_profiles()?
            .iter()
            .any(|profile| profile.id == observed.profile_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Deleted Agent Profile still appears in Host catalog",
            ));
        }
        Ok(())
    }

    pub fn launch_agent_profile(
        &self,
        profile_id: &str,
        workspace_id: &str,
        prompt: &str,
    ) -> io::Result<LaunchedAgentTab> {
        if !safe_runtime_id(profile_id, 128) || !safe_runtime_id(workspace_id, 256) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile or Host Workspace id",
            ));
        }
        let prompt = prompt.trim();
        if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES || prompt.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent prompt must be nonempty, NUL-free, and at most 8 KiB",
            ));
        }
        let response = self.invoke_existing_host_agent(AllowedAgentRequest::Launch {
            profile_id,
            workspace_id,
            prompt,
        })?;
        parse_launched_agent(&response, profile_id, workspace_id)
    }

    fn invoke_existing_host_agent(&self, request: AllowedAgentRequest<'_>) -> io::Result<Value> {
        let (exe, runtime) = self.agent_cli_paths();
        // No accidental launch of a Host when Classic is closed. Older CLI
        // versions without --no-start fail closed instead of starting one.
        if !runtime.join("host.json").is_file() && !runtime.join("runtime-host.json").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No running Runtime Host; Native will not start one automatically",
            ));
        }
        let stdin_payload = match &request {
            AllowedAgentRequest::List
            | AllowedAgentRequest::Show { .. }
            | AllowedAgentRequest::UpdateRouting { .. }
            | AllowedAgentRequest::CreateCommand { .. }
            | AllowedAgentRequest::Clone { .. }
            | AllowedAgentRequest::Rename { .. }
            | AllowedAgentRequest::Reorder { .. }
            | AllowedAgentRequest::RemovalImpact { .. }
            | AllowedAgentRequest::RemoveConfirmed { .. } => None,
            AllowedAgentRequest::Launch { prompt, .. } => Some(prompt.as_bytes()),
            AllowedAgentRequest::UpdateCommandPrompt { content, .. }
            | AllowedAgentRequest::UpdateManagedSelection { content, .. } => {
                Some(content.as_bytes())
            }
        };
        let timeout = if matches!(request, AllowedAgentRequest::List) {
            LIST_TIMEOUT
        } else {
            LAUNCH_TIMEOUT
        };
        let mut command = windowless_command(exe);
        command
            .args(args_for(runtime, &request))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(if stdin_payload.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if let Some(input) = stdin_payload {
            let sent = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("Agent stdin unavailable"))?
                .write_all(input);
            if let Err(error) = sent {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Agent stdout unavailable"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let exit = match wait_bounded(&mut child, timeout) {
            Ok(exit) => exit,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Agent stdout reader failed"))??;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Agent Profile CLI response exceeds 512 KiB",
            ));
        }
        if !exit.success() {
            return Err(io::Error::other(
                "Agent Profile CLI failed; verify the existing Host and the CLI supports --no-start",
            ));
        }
        serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_selection_uses_revision_guard_and_stdin_without_exposing_model_in_argv() {
        let root = Path::new("C:/isolated/runtime");
        let content = r#"{"model":"claude-new","persona":"reviewer"}"#;
        let argv = args_for(
            root,
            &AllowedAgentRequest::UpdateManagedSelection {
                id: "profile_m",
                expected_revision: 7,
                content,
            },
        );
        assert_eq!(argv[1], "--no-start");
        assert!(argv.iter().any(|part| part == "--profile-id=profile_m"));
        assert!(argv.iter().any(|part| part == "--expected-revision=7"));
        assert!(argv
            .iter()
            .any(|part| part == "--edit-managed-selection-stdin"));
        assert!(!argv.iter().any(|part| {
            part.to_string_lossy().contains("claude-new")
                || part.to_string_lossy().contains("reviewer")
                || part.to_string_lossy().contains("managedConfig")
        }));

        let dir = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(
            dir.path().join("missing-cli.exe"),
            &dir.path().join("runtime"),
        );
        let profile = AgentProfileRow {
            id: "profile_m".into(),
            name: "Managed".into(),
            agent_type: "claude".into(),
            managed: true,
            revision: Some(7),
        };
        assert_eq!(
            bridge
                .load_agent_profile_managed_selection(&profile)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .update_agent_profile_managed_selection(
                    &profile,
                    Some(Some("claude-new")),
                    Some(Some("reviewer"))
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert!(!dir.path().join("runtime").exists());
        let command_profile = AgentProfileRow {
            managed: false,
            ..profile.clone()
        };
        assert_eq!(
            bridge
                .update_agent_profile_managed_selection(
                    &command_profile,
                    Some(Some("claude-new")),
                    None
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        let no_revision = AgentProfileRow {
            revision: None,
            ..profile.clone()
        };
        assert_eq!(
            bridge
                .update_agent_profile_managed_selection(
                    &no_revision,
                    Some(Some("claude-new")),
                    None
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        let unsupported_persona = AgentProfileRow {
            agent_type: "codex".into(),
            ..profile
        };
        assert_eq!(
            bridge
                .update_agent_profile_managed_selection(
                    &unsupported_persona,
                    None,
                    Some(Some("reviewer"))
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn routing_update_requires_observed_revision_and_explicit_clear_flags() {
        let runtime = Path::new("C:/isolated/runtime");
        let show = args_for(runtime, &AllowedAgentRequest::Show { id: "prof_r1" });
        assert_eq!(show[1], "--no-start");
        assert!(show.iter().any(|arg| arg == "show"));
        let update = args_for(
            runtime,
            &AllowedAgentRequest::UpdateRouting {
                id: "prof_r1",
                expected_revision: 3,
                description: "Review QA",
                quota_group: "team-a",
            },
        );
        assert!(update.iter().any(|arg| arg == "--expected-revision=3"));
        assert!(update.iter().any(|arg| arg == "--description=Review QA"));
        assert!(update.iter().any(|arg| arg == "--quota-group=team-a"));
        let clear = args_for(
            runtime,
            &AllowedAgentRequest::UpdateRouting {
                id: "prof_r1",
                expected_revision: 4,
                description: "",
                quota_group: "",
            },
        );
        assert!(clear.iter().any(|arg| arg == "--clear-description"));
        assert!(clear.iter().any(|arg| arg == "--clear-quota-group"));
        assert!(!clear.iter().any(|arg| arg == "--command"));
        assert!(!clear.iter().any(|arg| arg == "--managed-config"));
        assert!(!clear.iter().any(|arg| arg == "--custom-prompt"));
    }

    #[test]
    fn clone_cli_request_uses_only_host_identity_revision_and_new_name() {
        let runtime = Path::new("C:/isolated/runtime");
        let args = args_for(
            runtime,
            &AllowedAgentRequest::Clone {
                id: "profile_1",
                expected_revision: 3,
                name: "Review Copy",
            },
        );
        assert_eq!(args[1], "--no-start");
        assert!(args.iter().any(|part| part == "clone"));
        assert!(args.iter().any(|part| part == "--profile-id=profile_1"));
        assert!(args.iter().any(|part| part == "--expected-revision=3"));
        assert!(args.iter().any(|part| part == "--name=Review Copy"));
        assert!(!args
            .iter()
            .any(|part| part.to_string_lossy().contains("managedConfig")));
        let dir = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(
            dir.path().join("missing-cli.exe"),
            &dir.path().join("runtime"),
        );
        let row = AgentProfileRow {
            id: "profile_1".into(),
            name: "Review".into(),
            agent_type: "codex".into(),
            managed: false,
            revision: Some(3),
        };
        assert_eq!(
            bridge
                .clone_agent_profile(&row, "Review Copy")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert!(!dir.path().join("runtime").exists());
    }

    #[test]
    fn profile_create_and_rename_are_explicit_no_start_and_revision_guarded() {
        let root = Path::new("C:/isolated/runtime");
        let create = args_for(
            root,
            &AllowedAgentRequest::CreateCommand {
                name: "Reviewer",
                agent_type: "codex",
                command: "codex",
            },
        );
        assert_eq!(create[1], "--no-start");
        assert!(create.iter().any(|arg| arg == "create"));
        assert!(create.iter().any(|arg| arg == "--launch-mode=command"));
        assert!(create.iter().any(|arg| arg == "--name=Reviewer"));
        assert!(create.iter().any(|arg| arg == "--agent-type=codex"));
        let rename = args_for(
            root,
            &AllowedAgentRequest::Rename {
                id: "profile_1",
                expected_revision: 3,
                name: "Review Updated",
            },
        );
        assert_eq!(rename[1], "--no-start");
        assert!(rename.iter().any(|arg| arg == "update"));
        assert!(rename.iter().any(|arg| arg == "--profile-id=profile_1"));
        assert!(rename.iter().any(|arg| arg == "--expected-revision=3"));
        assert!(rename.iter().any(|arg| arg == "--name=Review Updated"));
    }

    #[test]
    fn profile_reorder_sends_full_snapshot_and_each_expected_revision() {
        let rows = [
            AgentProfileRow {
                id: "profile_2".into(),
                name: "B".into(),
                agent_type: "claude".into(),
                managed: false,
                revision: Some(0),
            },
            AgentProfileRow {
                id: "profile_1".into(),
                name: "A".into(),
                agent_type: "codex".into(),
                managed: false,
                revision: Some(3),
            },
        ];
        let args = args_for(
            Path::new("C:/isolated/runtime"),
            &AllowedAgentRequest::Reorder { profiles: &rows },
        );
        assert_eq!(args[1], "--no-start");
        assert!(args.iter().any(|part| part == "reorder"));
        assert!(args.iter().any(|part| part == "--id=profile_2"));
        assert!(args.iter().any(|part| part == "--id=profile_1"));
        assert!(args
            .iter()
            .any(|part| part == "--expected-revision=profile_2=0"));
        assert!(args
            .iter()
            .any(|part| part == "--expected-revision=profile_1=3"));
    }

    #[test]
    fn profile_removal_requires_revision_no_start_and_explicit_confirmation() {
        let runtime = Path::new("C:/isolated/runtime");
        let impact = args_for(
            runtime,
            &AllowedAgentRequest::RemovalImpact {
                id: "prof_1",
                expected_revision: 4,
            },
        );
        assert_eq!(impact[1], "--no-start");
        assert!(impact.iter().any(|arg| arg == "removal-impact"));
        assert!(impact.iter().any(|arg| arg == "--expected-revision=4"));
        assert!(!impact.iter().any(|arg| arg == "--confirm"));
        let remove = args_for(
            runtime,
            &AllowedAgentRequest::RemoveConfirmed {
                id: "prof_1",
                expected_revision: 4,
            },
        );
        assert_eq!(remove[1], "--no-start");
        assert!(remove.iter().any(|arg| arg == "remove"));
        assert!(remove.iter().any(|arg| arg == "--profile-id=prof_1"));
        assert!(remove.iter().any(|arg| arg == "--expected-revision=4"));
        assert!(remove.iter().any(|arg| arg == "--confirm"));
    }

    #[test]
    fn agent_cli_uses_only_explicit_no_start_json_verbs_and_never_exposes_prompt_in_argv() {
        let runtime = Path::new("C:/isolated/runtime");
        let list = args_for(runtime, &AllowedAgentRequest::List);
        assert_eq!(list[0], "agent-profile");
        assert_eq!(list[1], "--no-start");
        assert_eq!(list.last().unwrap(), "list");
        let launch = args_for(
            runtime,
            &AllowedAgentRequest::Launch {
                profile_id: "profile_1",
                workspace_id: "workspace_2",
                prompt: "SECRET QUESTION",
            },
        );
        assert!(launch.iter().any(|part| part == "--prompt-stdin"));
        assert!(launch.iter().any(|part| part == "--profile-id=profile_1"));
        assert!(launch.iter().any(|part| part == "--workspace=workspace_2"));
        assert!(!launch
            .iter()
            .any(|part| part.to_string_lossy().contains("SECRET")));
    }

    #[test]
    fn no_host_never_executes_agent_or_creates_runtime_directory() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        let bridge = RuntimeCliBridge::new(dir.path().join("not-an-executable.exe"), &runtime);
        assert_eq!(
            bridge.list_agent_profiles().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .launch_agent_profile("p_1", "w_1", "Check work")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .create_command_agent_profile("Review", "codex")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .rename_agent_profile("p_1", 0, "Review Updated")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .create_command_agent_profile("  ", "codex")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .create_command_agent_profile("Review", "unknown-adapter")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .rename_agent_profile("p_1", -1, "Review")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!runtime.exists());
        assert_eq!(
            bridge
                .launch_agent_profile("../escape", "w_1", "Check work")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .launch_agent_profile("p_1", "w_1", " ")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn command_prompt_update_uses_stdin_flag_and_never_places_content_in_argv() {
        let runtime = Path::new("C:/isolated/runtime");
        let args = args_for(
            runtime,
            &AllowedAgentRequest::UpdateCommandPrompt {
                id: "prof_1",
                expected_revision: 7,
                content: r#"{"command":"codex --secret-flag","customPrompt":"PRIVATE PROMPT"}"#,
            },
        );
        assert!(args.iter().any(|arg| arg == "update"));
        assert!(args.iter().any(|arg| arg == "--no-start"));
        assert!(args.iter().any(|arg| arg == "--profile-id=prof_1"));
        assert!(args.iter().any(|arg| arg == "--expected-revision=7"));
        assert!(args.iter().any(|arg| arg == "--edit-content-stdin"));
        assert!(!args.iter().any(|arg| arg == "--command"));
        assert!(!args.iter().any(|arg| arg == "--custom-prompt"));
        assert!(!args.iter().any(|arg| arg == "--prompt-stdin"));
        for arg in &args {
            let text = arg.to_string_lossy();
            for secret in [
                "codex --secret-flag",
                "PRIVATE PROMPT",
                "customPrompt",
                "clearCustomPrompt",
            ] {
                assert!(!text.contains(secret), "argv leaked {secret}: {text}");
            }
        }
    }

    #[test]
    fn command_prompt_edit_json_matches_cli_stdin_schema() {
        let full = command_prompt_edit_json(
            Some("codex --x".to_string()),
            Some("Focus".to_string()),
            false,
        );
        let value: Value = serde_json::from_str(&full).unwrap();
        assert_eq!(value["command"], "codex --x");
        assert_eq!(value["customPrompt"], "Focus");
        assert!(value.get("clearCustomPrompt").is_none());

        let prompt_only = command_prompt_edit_json(None, Some("Focus".to_string()), false);
        let value: Value = serde_json::from_str(&prompt_only).unwrap();
        assert!(value.get("command").is_none());
        assert_eq!(value["customPrompt"], "Focus");

        let clear = command_prompt_edit_json(None, None, true);
        let value: Value = serde_json::from_str(&clear).unwrap();
        assert_eq!(value["clearCustomPrompt"], true);
        assert!(value.get("command").is_none());
        assert!(value.get("customPrompt").is_none());
    }

    #[test]
    fn command_prompt_update_validates_inputs_before_host_access() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        let bridge = RuntimeCliBridge::new(dir.path().join("missing-cli.exe"), &runtime);
        let row = |managed: bool, revision: Option<i64>| AgentProfileRow {
            id: "prof_1".into(),
            name: "Review".into(),
            agent_type: "codex".into(),
            managed,
            revision,
        };
        let invalid = |result: io::Result<AgentProfileRow>| {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput)
        };

        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            None,
            None,
            false,
        ));
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            Some("   "),
            None,
            false,
        ));
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            Some("bad\0command"),
            None,
            false,
        ));
        let big_command = "c".repeat(MAX_EDIT_COMMAND_BYTES + 1);
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            Some(big_command.as_str()),
            None,
            false,
        ));
        invalid(bridge.update_agent_profile_command_prompt(
            &row(true, Some(3)),
            Some("codex"),
            None,
            false,
        ));
        let big_prompt = "p".repeat(MAX_PROMPT_BYTES + 1);
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            None,
            Some(big_prompt.as_str()),
            false,
        ));
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, Some(3)),
            None,
            Some("Focus"),
            true,
        ));
        invalid(bridge.update_agent_profile_command_prompt(
            &row(false, None),
            Some("codex"),
            None,
            false,
        ));
        let mut bad_id = row(false, Some(3));
        bad_id.id = "../escape".into();
        invalid(bridge.update_agent_profile_command_prompt(&bad_id, Some("codex"), None, false));

        // Valid edits reach the existing-Host gate and fail closed without
        // starting a Host or creating the runtime directory.
        let valid_cases: [(bool, Option<&str>, Option<&str>, bool); 3] = [
            (false, Some("codex --x"), None, false),
            (false, None, Some("Focus"), false),
            (true, None, None, true),
        ];
        for (managed, command, prompt, clear) in valid_cases {
            assert_eq!(
                bridge
                    .update_agent_profile_command_prompt(
                        &row(managed, Some(3)),
                        command,
                        prompt,
                        clear,
                    )
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::NotConnected
            );
        }
        assert!(!runtime.exists());
    }
}
