//! Native read-only AI Commit bridge. The Alera CLI alone authenticates to
//! the existing Host and runs the Host-configured, allowlisted Agent.
//! Native never accesses the Host token or runs arbitrary commands.
use crate::runtime_bridge::RuntimeCliBridge;
use alera_core::child_process::windowless_command;
use alera_core::git::{verify_staged_commit_prompt, GitStagedCommitPrompt};
use serde::Deserialize;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const MAX_JSON_BYTES: u64 = 80 * 1024;
const WAIT_STEP: Duration = Duration::from_millis(40);
const MAX_CLI_WAIT: Duration = Duration::from_secs(650);
// Allow the CLI's Windows taskkill descendant sweep (up to four seconds)
// to finish before force-killing its controlling process.
const CANCEL_GRACE: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedAiCommit {
    pub text: String,
    pub agent_label: String,
    pub head_oid: String,
    pub index_tree_oid: String,
}

fn args_for(runtime: &Path, root: &Path, plan: &GitStagedCommitPrompt) -> Vec<OsString> {
    vec![
        "ai-assist".into(),
        "--no-start".into(),
        "--json".into(),
        "--runtime-dir".into(),
        runtime.as_os_str().to_os_string(),
        "commit-message".into(),
        "--workspace".into(),
        root.as_os_str().to_os_string(),
        "--expected-branch".into(),
        plan.branch.clone().into(),
        "--expected-head".into(),
        plan.head_oid.clone().into(),
        "--expected-index".into(),
        plan.index_tree_oid.clone().into(),
    ]
}

impl RuntimeCliBridge {
    /// Synchronous worker call, NEVER call this from a GPUI render callback.
    /// Closing the stdin lease requests clean cancellation inside the CLI,
    /// which in turn kills and reaps the Agent process.
    pub fn generate_ai_commit(
        &self,
        workspace: &Path,
        preview: &GitStagedCommitPrompt,
        canceled: &AtomicBool,
    ) -> io::Result<GeneratedAiCommit> {
        if canceled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "AI Commit cancelled",
            ));
        }
        verify_staged_commit_prompt(workspace, preview)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        let (exe, runtime) = self.agent_cli_paths();
        if !runtime.join("host.json").is_file() && !runtime.join("runtime-host.json").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Classic Runtime Host is offline; Native AI Commit will not start it",
            ));
        }
        let mut command = windowless_command(exe);
        command
            .args(args_for(runtime, workspace, preview))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        // Retain stdin while running: the CLI treats its closure as a lease
        // revocation and stops the Agent, including on workspace switches.
        let mut lease = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("AI Commit CLI has no stdout pipe"))?;
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.take(MAX_JSON_BYTES + 1).read_to_end(&mut bytes)?;
            Ok::<_, io::Error>(bytes)
        });
        let start = Instant::now();
        let mut cancelled_since = None;
        let exit = loop {
            if canceled.load(Ordering::Acquire) || start.elapsed() > MAX_CLI_WAIT {
                if cancelled_since.is_none() {
                    // Dropping the pipe lets the CLI gracefully terminate its
                    // managed Agent before the child CLI itself is stopped.
                    lease.take();
                    cancelled_since = Some(Instant::now());
                } else if cancelled_since.is_some_and(|at| at.elapsed() > CANCEL_GRACE) {
                    let _ = child.kill();
                    break child.wait()?;
                }
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            thread::sleep(WAIT_STEP);
        };
        drop(lease);
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("AI Commit CLI output reader panicked"))??;
        if canceled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "AI Commit cancelled",
            ));
        }
        if start.elapsed() > MAX_CLI_WAIT {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "AI Commit exceeded time limit",
            ));
        }
        if !exit.success() {
            return Err(io::Error::other(
                "AI Commit CLI refused generation; check running Host, Classic AI Assist settings, and Agent login",
            ));
        }
        if bytes.len() as u64 > MAX_JSON_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "AI Commit response is oversized",
            ));
        }
        let output: GeneratedAiCommit = serde_json::from_slice(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if output.head_oid != preview.head_oid
            || output.index_tree_oid != preview.index_tree_oid
            || output.text.trim().is_empty()
            || output.text.len() > 4096
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Stale or invalid AI Commit response",
            ));
        }
        verify_staged_commit_prompt(workspace, preview)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn preview() -> GitStagedCommitPrompt {
        GitStagedCommitPrompt {
            prompt: "SECRET PATCH".into(),
            staged_summary: "private".into(),
            branch: "feature".into(),
            staged_files: 1,
            head_oid: "1".repeat(40),
            index_tree_oid: "2".repeat(40),
            patch_truncated: false,
        }
    }

    #[test]
    fn cli_argv_never_exposes_staged_patch_or_an_arbitrary_agent_command() {
        let args = args_for(Path::new("C:/runtime"), Path::new("C:/repo"), &preview());
        let text = args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("ai-assist --no-start --json"));
        assert!(text.contains("--expected-head"));
        assert!(text.contains("--expected-branch feature"));
        assert!(text.contains("--expected-index"));
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("custom"));
        assert!(!text.contains("codex"));
        assert!(!text.contains("claude"));
    }

    #[test]
    fn cancelled_generation_rejects_before_touching_host_or_cli() {
        let root = PathBuf::from("not-a-workspace");
        let runtime = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(root.join("missing.exe"), runtime.path());
        let cancelled = AtomicBool::new(true);
        let error = bridge
            .generate_ai_commit(&root, &preview(), &cancelled)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(std::fs::read_dir(runtime.path()).unwrap().next().is_none());
    }
}
