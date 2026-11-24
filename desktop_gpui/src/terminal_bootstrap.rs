//! Explicit Host Terminal / Agent launch workspace registration for Alera Native.
//!
//! Passive startup, polling, reads and writes still use the no-start CLI
//! bridge. Only explicit New Terminal, New Agent, Profile Launch or saved-tab restore may
//! start Host and register the selected *existing* checkout. Registration
//! changes the Host inventory, not Git branches, files or worktrees.

use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;
use std::time::Duration;

use crate::runtime_bridge::{wait_with_timeout, RuntimeCliBridge, RuntimeStatus};
use crate::terminal::{select_registered_host_workspace, CreatedHostTerminal};

const HOST_START_TIMEOUT: Duration = Duration::from_secs(25);
const MANAGEMENT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MANAGEMENT_RESPONSE: u64 = 256 * 1024;
const MAX_PROJECTS: usize = 256;

#[derive(Debug)]
pub enum HostCreateFailure {
    NotSubmitted(io::Error),
    OutcomeUnknown(io::Error),
}

impl RuntimeCliBridge {
    /// Distinguish preflight failures from a potentially submitted PTY create.
    pub fn create_workbench_terminal_tracked(
        &self,
        root: &Path,
        agent: Option<(&str, &str)>,
    ) -> Result<CreatedHostTerminal, HostCreateFailure> {
        let workspace_id = self
            .ensure_host_workspace_for_explicit_terminal_inner(root, true)
            .map_err(HostCreateFailure::NotSubmitted)?;
        match agent {
            Some((id, executable)) => self.create_agent_terminal(&workspace_id, id, executable),
            None => self.create_terminal(&workspace_id),
        }
        .map_err(HostCreateFailure::OutcomeUnknown)
    }

    /// Called exclusively after the user requests New Terminal. The CLI,
    /// rather than GPUI, owns Host launch, authentication and the PTY.
    ///
    /// Never register arbitrary paths: the exact active root must be an
    /// existing checkout of one unique Runtime project. No Git worktree
    /// creation or deletion is permitted by this path.
    pub fn create_terminal_for_workspace(&self, root: &Path) -> io::Result<CreatedHostTerminal> {
        let workspace_id = self.ensure_host_workspace_for_explicit_terminal(root)?;
        self.create_terminal(&workspace_id)
    }

    /// Explicit New Agent action shares Host start/registered-checkout guards.
    pub fn create_agent_terminal_for_workspace(
        &self,
        root: &Path,
        agent_id: &str,
        executable: &str,
    ) -> io::Result<CreatedHostTerminal> {
        let workspace_id = self.ensure_host_workspace_for_explicit_terminal(root)?;
        self.create_agent_terminal(&workspace_id, agent_id, executable)
    }

    /// An explicit Host Agent Profile launch needs an authenticated, registered
    /// workspace even when the user has not opened a Terminal yet. Reuse the
    /// exact existing-project registration guard; do not create a spare PTY.
    pub fn ensure_host_workspace_for_explicit_profile_launch(
        &self,
        root: &Path,
    ) -> io::Result<String> {
        self.ensure_host_workspace_for_explicit_terminal(root)
    }

    /// Explicit user action from Native's New Tab menu. Passive app startup
    /// remains no-start; opening a saved layout never silently launches every
    /// persisted Host PTY. Host reconciliation will reuse the existing tab IDs
    /// and invoke each provider's exact-session resume logic.
    pub fn restore_saved_host_tabs_explicitly(&self) -> io::Result<()> {
        if self.probe()? != RuntimeStatus::Stopped {
            return Ok(());
        }
        let (_, runtime) = self.agent_cli_paths();
        let args = vec![
            OsString::from("runtime"),
            OsString::from("--json"),
            OsString::from("--runtime-dir"),
            runtime.as_os_str().to_owned(),
            OsString::from("start"),
        ];
        self.start_runtime_host(&args)
    }

    fn ensure_host_workspace_for_explicit_terminal(&self, root: &Path) -> io::Result<String> {
        self.ensure_host_workspace_for_explicit_terminal_inner(root, false)
    }

    fn ensure_host_workspace_for_explicit_terminal_inner(
        &self,
        root: &Path,
        register_local_project: bool,
    ) -> io::Result<String> {
        let active = dunce::canonicalize(root)?;
        if active != root || !active.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Host launch requires a canonical existing workspace",
            ));
        }
        if self.probe()? == RuntimeStatus::Stopped {
            // Only a user-requested Host launch may reach this start gate.
            let (_, runtime) = self.agent_cli_paths();
            let args = vec![
                OsString::from("runtime"),
                OsString::from("--json"),
                OsString::from("--runtime-dir"),
                runtime.as_os_str().to_owned(),
                OsString::from("start"),
            ];
            self.start_runtime_host(&args)?;
        }
        let registered = self.list_registered_workspaces()?;
        let workspace_id = match select_registered_host_workspace(&active, &registered) {
            Ok(id) => id,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.register_existing_host_workspace(&active, register_local_project)?
            }
            Err(error) => return Err(error),
        };
        Ok(workspace_id)
    }

    /// Explicit runtime start can detach a persistent Host that retains the
    /// original stdout pipe handles on Windows. Never wait for stdout EOF
    /// after the parent CLI exits: that deadlocks even though Host is ready.
    fn start_runtime_host(&self, args: &[OsString]) -> io::Result<()> {
        let (exe, _) = self.agent_cli_paths();
        let mut child = windowless_command(exe)
            .args(args)
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let status = wait_with_timeout(&mut child, HOST_START_TIMEOUT)?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "Alera Runtime Host could not start ({status})"
            )));
        }
        Ok(())
    }

    fn register_existing_host_workspace(&self, root: &Path, register_local_project: bool) -> io::Result<String> {
        let (_, runtime) = self.agent_cli_paths();
        let args = vec![
            OsString::from("project"),
            OsString::from("--json"),
            OsString::from("--runtime-dir"),
            runtime.as_os_str().to_owned(),
            OsString::from("list"),
        ];
        let projects = self.management_json(&args, MANAGEMENT_TIMEOUT)?;
        let (project_id, main_checkout) = match matching_existing_project(root, &projects) {
            Ok(found) => found,
            Err(error) if register_local_project && error.kind() == io::ErrorKind::NotFound => {
                // A Native-only project has not yet been added to the Host.
                // Register only an explicit, existing main checkout or folder.
                let kind = match git2::Repository::discover(root) {
                    Ok(repo) => {
                        let main_root = repo.commondir().parent()
                            .and_then(|path| dunce::canonicalize(path).ok());
                        if main_root.as_deref() != Some(root) {
                            return Err(invalid("Register the main Git project before launching a linked worktree"));
                        }
                        "git-repository"
                    }
                    Err(_) => "folder",
                };
                let name = root.file_name().and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty() && name.len() <= 128)
                    .ok_or_else(|| invalid("Invalid project name"))?;
                let add_args = vec![
                    OsString::from("project"),
                    OsString::from("--json"),
                    OsString::from("--runtime-dir"),
                    runtime.as_os_str().to_owned(),
                    OsString::from("add"),
                    OsString::from("--name"),
                    OsString::from(name),
                    OsString::from("--repo-path"),
                    root.as_os_str().to_owned(),
                    OsString::from("--kind"),
                    OsString::from(kind),
                ];
                self.management_json(&add_args, MANAGEMENT_TIMEOUT)?;
                let projects = self.management_json(&args, MANAGEMENT_TIMEOUT)?;
                matching_existing_project(root, &projects)?
            }
            Err(error) => return Err(error),
        };
        // Project add may already have registered the main workspace.
        match select_registered_host_workspace(root, &self.list_registered_workspaces()?) {
            Ok(id) => return Ok(id),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && name.len() <= 128)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid workspace name"))?;
        let mut args = vec![
            OsString::from("workspace"),
            OsString::from("--json"),
            OsString::from("--runtime-dir"),
            runtime.as_os_str().to_owned(),
            OsString::from("register"),
            OsString::from("--project-id"),
            OsString::from(project_id.clone()),
            OsString::from("--name"),
            OsString::from(name),
            OsString::from("--path"),
            root.as_os_str().to_owned(),
            OsString::from("--kind"),
            OsString::from(if main_checkout { "main" } else { "linked" }),
        ];
        if let Ok(repo) = git2::Repository::open(root) {
            if let Ok(head) = repo.head() {
                if let Some(branch) = head
                    .shorthand()
                    .ok()
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                {
                    args.push(OsString::from("--branch"));
                    args.push(OsString::from(branch));
                    if !main_checkout {
                        args.push(OsString::from("--reuse-existing-branch"));
                    }
                }
            }
        }
        let response = self.management_json(&args, MANAGEMENT_TIMEOUT)?;
        validate_registration(root, &project_id, &response)?;
        // A successful CLI reply alone never authorizes PTY creation. Verify
        // the Host's authenticated inventory contains the exact pathname.
        select_registered_host_workspace(root, &self.list_registered_workspaces()?)
    }

    /// Use only fixed, internal management verbs. Never accept free-form
    /// commands or shell text from the UI; stdout is size/time bounded and
    /// CLI stderr is intentionally not forwarded (may include Host details).
    fn management_json(&self, args: &[OsString], timeout: Duration) -> io::Result<Value> {
        let (exe, _) = self.agent_cli_paths();
        let mut child = windowless_command(exe)
            .args(args)
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("CLI stdout pipe unavailable"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_MANAGEMENT_RESPONSE + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, timeout) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let output = reader
            .join()
            .map_err(|_| io::Error::other("CLI response reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other(format!(
                "Alera CLI management request failed ({status}); check CLI compatibility and Runtime Host"
            )));
        }
        if output.len() as u64 > MAX_MANAGEMENT_RESPONSE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Alera CLI management output exceeds limit",
            ));
        }
        serde_json::from_slice(&output).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid CLI JSON: {error}"),
            )
        })
    }
}

/// Identify an existing registered project, never construct a Git worktree.
/// A linked worktree's Git common directory points back to the registered
/// main repository even when the worktree lives outside its parent folder.
fn matching_existing_project(root: &Path, inventory: &Value) -> io::Result<(String, bool)> {
    let items = inventory
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| items.len() <= MAX_PROJECTS)
        .ok_or_else(|| invalid("Invalid project inventory from Alera CLI"))?;
    let repo = git2::Repository::discover(root).ok();
    let git_checkout = repo
        .as_ref()
        .and_then(|repo| repo.workdir())
        .and_then(|path| dunce::canonicalize(path).ok());
    if let Some(checkout) = git_checkout.as_deref() {
        if checkout != root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "New Terminal must use the Git workspace root, not a subfolder",
            ));
        }
    }
    let common_root = repo
        .as_ref()
        .and_then(|repo| repo.commondir().parent())
        .and_then(|path| dunce::canonicalize(path).ok());
    let mut match_project: Option<(String, bool)> = None;
    for item in items {
        let Some(id) = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        else {
            continue;
        };
        let Some(path) = item.get("repoPath").and_then(Value::as_str) else {
            continue;
        };
        let raw = PathBuf::from(path);
        if !raw.is_absolute() {
            continue;
        }
        let Ok(project_path) = dunce::canonicalize(&raw) else {
            continue;
        };
        let same_project =
            project_path == root || common_root.as_deref() == Some(project_path.as_path());
        if !same_project {
            continue;
        }
        if match_project.is_some() {
            return Err(invalid(
                "Multiple registered projects match the current checkout",
            ));
        }
        match_project = Some((id.to_owned(), project_path == root));
    }
    match_project.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Current Git project is not registered in the Alera Runtime; add the project first",
        )
    })
}

fn validate_registration(root: &Path, project_id: &str, reply: &Value) -> io::Result<()> {
    let valid = reply
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(valid_id)
        && reply.get("projectId").and_then(Value::as_str) == Some(project_id)
        && reply
            .get("path")
            .and_then(Value::as_str)
            .and_then(|path| dunce::canonicalize(path).ok())
            .as_deref()
            == Some(root);
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "Registered workspace identity/path did not match the requested checkout",
        ))
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_profile_launch_rejects_a_missing_checkout_without_host_start() {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        let bridge = RuntimeCliBridge::new("missing-alera-cli", &runtime);
        let result = bridge.ensure_host_workspace_for_explicit_profile_launch(
            &root.path().join("missing-project"),
        );
        assert!(result.is_err());
        assert!(!runtime.exists());
    }

    #[test]
    fn failed_preflight_never_submits_a_terminal_or_agent() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("runtime");
        let bridge = RuntimeCliBridge::new("missing-alera-cli", &runtime);
        let missing = temp.path().join("missing-project");
        for agent in [None, Some(("agy", "missing-agy"))] {
            assert!(matches!(
                bridge.create_workbench_terminal_tracked(&missing, agent),
                Err(HostCreateFailure::NotSubmitted(_))
            ));
        }
        assert!(!runtime.exists());
    }

    #[test]
    fn current_checkout_only_matches_exact_registered_project() {
        let repo_root = tempfile::tempdir().unwrap();
        git2::Repository::init(repo_root.path()).unwrap();
        let root = dunce::canonicalize(repo_root.path()).unwrap();
        let inventory = json!({"items":[
            {"id":"good-project","repoPath":root},
            {"id":"unrelated","repoPath":"Z:/missing"},
        ]});
        assert_eq!(
            matching_existing_project(&root, &inventory).unwrap(),
            ("good-project".into(), true)
        );
        let other = tempfile::tempdir().unwrap();
        let other = dunce::canonicalize(other.path()).unwrap();
        assert_eq!(
            matching_existing_project(&other, &inventory)
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn duplicate_project_or_mismatched_registration_is_refused() {
        let root = tempfile::tempdir().unwrap();
        git2::Repository::init(root.path()).unwrap();
        let root = dunce::canonicalize(root.path()).unwrap();
        let inventory = json!({"items":[
            {"id":"first","repoPath":root},
            {"id":"second","repoPath":root},
        ]});
        assert_eq!(
            matching_existing_project(&root, &inventory)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(validate_registration(
            &root,
            "project",
            &json!({"id":"ws-1","projectId":"project","path":root})
        )
        .is_ok());
        assert!(validate_registration(
            &root,
            "project",
            &json!({"id":"ws-1","projectId":"other","path":root})
        )
        .is_err());
    }

    #[test]
    fn linked_worktree_matches_original_project_without_creating_another_worktree() {
        let main_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(main_dir.path()).unwrap();
        let signature = git2::Signature::now("Alera test", "test@local.invalid").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
            .unwrap();
        let linked_dir = tempfile::tempdir().unwrap();
        let linked_path = linked_dir.path().join("existing-worktree");
        repo.worktree("existing-worktree", &linked_path, None)
            .unwrap();
        let existing = dunce::canonicalize(&linked_path).unwrap();
        let main = dunce::canonicalize(main_dir.path()).unwrap();
        let inventory = json!({"items":[{"id":"main-project","repoPath":main}]});
        assert_eq!(
            matching_existing_project(&existing, &inventory).unwrap(),
            ("main-project".into(), false)
        );
        assert!(linked_path.join(".git").exists());
    }
}
