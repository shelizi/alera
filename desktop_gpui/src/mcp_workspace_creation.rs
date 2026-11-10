//! Create an Alera-managed Workspace for an already Host-authorized MCP UI
//! request. The existing CLI/Host owns Git worktree setup, database rows and
//! project policy. Native does not run Git mutations or construct paths.
use crate::runtime_bridge::RuntimeCliBridge;
use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const RESPONSE_MAX_BYTES: u64 = 128 * 1024;
const CREATE_TIMEOUT: Duration = Duration::from_secs(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostManagedWorkspaceRequest {
    pub project_id: String,
    pub new_branch: String,
    pub source_branch: String,
    pub name: Option<String>,
    pub parent_workspace_id: Option<String>,
    pub reuse_existing_branch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostManagedWorkspaceResult {
    pub id: String,
    pub project_id: String,
    pub path: PathBuf,
}

fn valid_host_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn valid_branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.ends_with(['.', '/'])
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
}

impl HostManagedWorkspaceRequest {
    pub fn from_host_arguments(arguments: &Value) -> io::Result<Self> {
        let args = arguments
            .as_object()
            .ok_or_else(|| invalid("Workspace arguments must be an object"))?;
        let read = |key: &str| {
            args.get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Missing Host Workspace creation field"))
        };
        let project_id = read("projectId")?.to_owned();
        let new_branch = read("newBranchName")?.to_owned();
        let source_branch = read("sourceBranch")?.to_owned();
        if !valid_host_id(&project_id)
            || !valid_branch(&new_branch)
            || !valid_branch(&source_branch)
        {
            return Err(invalid("Host Workspace creation IDs or refs are invalid"));
        }
        let parent_workspace_id = args
            .get("parentWorkspaceId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if parent_workspace_id
            .as_deref()
            .is_some_and(|id| !valid_host_id(id))
        {
            return Err(invalid("Invalid parent Workspace ID"));
        }
        let name = args.get("name").and_then(Value::as_str).map(str::to_owned);
        if name.as_deref().is_some_and(|text| {
            text.is_empty() || text.len() > 128 || text.contains(['\0', '\n', '\r'])
        }) {
            return Err(invalid("Invalid Workspace display name"));
        }
        let reuse_existing_branch = args
            .get("reuseExistingBranch")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            project_id,
            new_branch,
            source_branch,
            name,
            parent_workspace_id,
            reuse_existing_branch,
        })
    }

    fn cli_args(&self, runtime: &Path) -> Vec<std::ffi::OsString> {
        use std::ffi::OsString;
        let mut args = vec![
            OsString::from("workspace"),
            OsString::from("--runtime-dir"),
            runtime.as_os_str().to_owned(),
            OsString::from("--json"),
            OsString::from("add"),
            OsString::from("--project-id"),
            OsString::from(&self.project_id),
            OsString::from("--branch"),
            OsString::from(&self.new_branch),
            OsString::from("--source-branch"),
            OsString::from(&self.source_branch),
        ];
        if self.reuse_existing_branch {
            args.push(OsString::from("--reuse-existing-branch"));
        }
        if let Some(name) = &self.name {
            args.extend([OsString::from("--name"), OsString::from(name)]);
        }
        if let Some(parent) = &self.parent_workspace_id {
            args.extend([
                OsString::from("--parent-workspace-id"),
                OsString::from(parent),
            ]);
        }
        args
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse_created_workspace(
    value: &Value,
    expected: &HostManagedWorkspaceRequest,
) -> io::Result<HostManagedWorkspaceResult> {
    let data = value
        .get("workspace")
        .ok_or_else(|| invalid("CLI did not return a managed Workspace"))?;
    let id = data
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing Workspace id"))?;
    let project = data
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing Workspace Project"))?;
    let branch = data
        .get("branch")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing Workspace branch"))?;
    let path = data
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing Workspace path"))?;
    if !valid_host_id(id)
        || project != expected.project_id
        || branch != expected.new_branch
        || data.get("status").and_then(Value::as_str) != Some("active")
        || data.get("kind").and_then(Value::as_str) != Some("linked")
    {
        return Err(invalid(
            "Host created Workspace with unexpected identity or status",
        ));
    }
    let path =
        dunce::canonicalize(path).map_err(|_| invalid("Created Workspace path is missing"))?;
    if !path.is_dir() {
        return Err(invalid("Created Workspace is not a directory"));
    }
    Ok(HostManagedWorkspaceResult {
        id: id.to_owned(),
        project_id: project.to_owned(),
        path,
    })
}

fn wait_bounded(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(40)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(io::ErrorKind::TimedOut,
                    "Host Workspace creation may still be running. Inspect existing workspaces before retrying"));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

impl RuntimeCliBridge {
    /// Never autostart a Host. A `workspace add` always delegates to the
    /// existing Host-managed creation workflow and respects project policy.
    pub fn create_mcp_workspace(
        &self,
        request: &HostManagedWorkspaceRequest,
    ) -> io::Result<HostManagedWorkspaceResult> {
        let (exe, runtime) = self.agent_cli_paths();
        if !runtime.join("host.json").is_file() && !runtime.join("runtime-host.json").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No existing Host for managed Workspace creation",
            ));
        }
        let mut command = windowless_command(exe);
        let mut child = command
            .args(request.cli_args(runtime))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Workspace CLI stdout unavailable"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(RESPONSE_MAX_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_bounded(&mut child, CREATE_TIMEOUT) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Workspace CLI reader failed"))??;
        if bytes.len() as u64 > RESPONSE_MAX_BYTES {
            return Err(invalid("Host Workspace response too large"));
        }
        if !status.success() {
            return Err(io::Error::other(
                "Host Workspace creation failed; verify existing workspaces before retrying",
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let created = parse_created_workspace(&response, request)?;
        let registered = self.list_registered_workspaces()?;
        if !registered
            .iter()
            .any(|item| item.id == created.id && item.path == created.path)
        {
            return Err(io::Error::other(
                "Created Workspace was not corroborated by the current authenticated Host listing",
            ));
        }
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn managed_creation_args_are_restricted_and_do_not_override_workspace_root() {
        let req = HostManagedWorkspaceRequest::from_host_arguments(&json!({
            "projectId":"project_1", "newBranchName":"feature/native", "sourceBranch":"main",
            "parentWorkspaceId":"ws_1", "name":"MCP Native test"
        }))
        .unwrap();
        let args = req.cli_args(Path::new("C:/fixture/host"));
        assert!(args.contains(&"--project-id".into()));
        assert!(args.contains(&"--source-branch".into()));
        assert!(!args
            .iter()
            .any(|value| value.to_string_lossy().contains("--workspace-root")));
        assert!(HostManagedWorkspaceRequest::from_host_arguments(
            &json!({"projectId":"project_1", "newBranchName":"../escape", "sourceBranch":"main"})
        )
        .is_err());
    }

    #[test]
    fn missing_host_disallows_workspace_mutation_and_new_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("absent-runtime");
        let req = HostManagedWorkspaceRequest::from_host_arguments(&json!({
            "projectId":"project_1", "newBranchName":"feature/native", "sourceBranch":"main"
        }))
        .unwrap();
        let bridge = RuntimeCliBridge::new(temp.path().join("not-found.exe"), &runtime);
        assert_eq!(
            bridge.create_mcp_workspace(&req).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert!(!runtime.exists());
    }

    #[test]
    fn created_workspace_readback_rejects_wrong_project_branch_and_nonactive_state() {
        let dir = tempfile::tempdir().unwrap();
        let request = HostManagedWorkspaceRequest::from_host_arguments(&json!({
            "projectId":"project_1", "newBranchName":"feature/native", "sourceBranch":"main"
        }))
        .unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let response = json!({"workspace":{
            "id":"workspace_new", "projectId":"project_1", "branch":"feature/native",
            "path":path, "status":"active", "kind":"linked"
        }});
        let result = parse_created_workspace(&response, &request).unwrap();
        assert_eq!(result.id, "workspace_new");
        assert_eq!(result.path, dunce::canonicalize(dir.path()).unwrap());
        let mut wrong_project = response.clone();
        wrong_project["workspace"]["projectId"] = json!("other_project");
        assert!(parse_created_workspace(&wrong_project, &request).is_err());
        let mut inactive = response.clone();
        inactive["workspace"]["status"] = json!("archived");
        assert!(parse_created_workspace(&inactive, &request).is_err());
    }
}
