use std::path::{Path, PathBuf};
use std::process::Output;

use alera_core::child_process::windowless_command;
use serde_json::Value;

struct RuntimeGuard {
    runtime_dir: PathBuf,
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        let _ = windowless_command(env!("CARGO_BIN_EXE_alera"))
            .args([
                "runtime",
                "--runtime-dir",
                self.runtime_dir.to_str().unwrap(),
                "stop",
                "--force",
            ])
            .output();
    }
}

fn run(runtime_dir: &Path, args: &[&str]) -> Output {
    windowless_command(env!("CARGO_BIN_EXE_alera"))
        .arg("agent-profile")
        .arg("--runtime-dir")
        .arg(runtime_dir)
        .arg("--json")
        .args(args)
        .output()
        .expect("failed to run alera agent-profile")
}

fn success_json(runtime_dir: &Path, args: &[&str]) -> Value {
    let output = run(runtime_dir, args);
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("command did not return JSON")
}

/// Start the disposable Host without a captured stdout pipe: on Windows a
/// detached Host may inherit child handles and block CLI auto-start tests.
fn start_isolated_runtime(runtime_dir: &Path) {
    let status = windowless_command(env!("CARGO_BIN_EXE_alera"))
        .arg("runtime")
        .arg("--runtime-dir")
        .arg(runtime_dir)
        .arg("start")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("could not start test Runtime Host");
    assert!(status.success(), "disposable Host must start explicitly");
}

#[test]
fn agent_profile_cli_manages_the_complete_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let _guard = RuntimeGuard {
        runtime_dir: temp.path().to_path_buf(),
    };
    start_isolated_runtime(temp.path());

    let first = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Codex Sol",
            "--agent-type",
            "codex",
            "--launch-mode",
            "command",
            "--command",
            "codex --search",
            "--quota-group",
            "personal",
        ],
    );
    let first_id = first["id"].as_str().unwrap().to_string();
    assert_eq!(first["revision"], 0);

    let risky = run(
        temp.path(),
        &[
            "create",
            "--name",
            "Managed Codex",
            "--agent-type",
            "codex",
            "--launch-mode",
            "managed",
            "--managed-config",
            r#"{"approvalPolicy":"never"}"#,
        ],
    );
    assert!(!risky.status.success());
    assert!(String::from_utf8_lossy(&risky.stderr).contains("--confirm-reduced-protections"));

    let second = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Managed Codex",
            "--agent-type",
            "codex",
            "--launch-mode",
            "managed",
            "--managed-config",
            r#"{"approvalPolicy":"never"}"#,
            "--confirm-reduced-protections",
        ],
    );
    let second_id = second["id"].as_str().unwrap().to_string();

    let updated = success_json(
        temp.path(),
        &[
            "update",
            "--profile-name",
            "codex sol",
            "--expected-revision",
            "0",
            "--description",
            "Backend implementation",
            "--clear-quota-group",
        ],
    );
    assert_eq!(updated["revision"], 1);
    assert_eq!(updated["description"], "Backend implementation");
    assert!(updated["quotaGroup"].is_null());

    let stale = run(
        temp.path(),
        &[
            "update",
            "--profile-id",
            &first_id,
            "--expected-revision",
            "0",
            "--description",
            "Stale update",
        ],
    );
    assert!(!stale.status.success());
    let stale_stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(
        stale_stderr.contains("revision conflict"),
        "stale update stderr: {stale_stderr}"
    );

    let reordered = success_json(
        temp.path(),
        &["reorder", "--id", &second_id, "--id", &first_id],
    );
    assert_eq!(reordered["items"][0]["id"], second_id);
    assert_eq!(reordered["items"][1]["id"], first_id);

    let first_revision = reordered["items"][1]["revision"].as_i64().unwrap();
    let first_revision_text = first_revision.to_string();
    let impact = success_json(
        temp.path(),
        &[
            "removal-impact",
            "--profile-id",
            &first_id,
            "--expected-revision",
            &first_revision_text,
        ],
    );
    assert_eq!(impact["blockingReferenceCount"], 0);

    let removed = success_json(
        temp.path(),
        &[
            "remove",
            "--profile-id",
            &first_id,
            "--expected-revision",
            &first_revision_text,
            "--confirm",
        ],
    );
    assert_eq!(removed["removed"], true);

    let listed = success_json(temp.path(), &["list"]);
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed["items"][0]["id"], second_id);
}
// A CLI clone must reuse the complete Host-owned profile without putting
// commands, prompts or Managed settings into the Native GUI or command argv.
#[test]
fn agent_profile_clone_preserves_settings_and_refuses_stale_or_unconfirmed_risk() {
    let temp = tempfile::tempdir().unwrap();
    let _guard = RuntimeGuard {
        runtime_dir: temp.path().to_path_buf(),
    };
    start_isolated_runtime(temp.path());
    let original = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Source",
            "--agent-type",
            "codex",
            "--launch-mode",
            "command",
            "--command",
            "codex --sandbox workspace-write",
            "--custom-prompt",
            "Only review this project",
            "--description",
            "QA",
            "--quota-group",
            "group-a",
        ],
    );
    let source_id = original["id"].as_str().unwrap();
    let cloned = success_json(
        temp.path(),
        &[
            "clone",
            "--profile-id",
            source_id,
            "--expected-revision",
            "0",
            "--name",
            "Source Copy",
        ],
    );
    assert_ne!(cloned["id"], original["id"]);
    assert_eq!(cloned["revision"], 0);
    for key in [
        "agentType",
        "launchMode",
        "command",
        "customPrompt",
        "description",
        "quotaGroup",
    ] {
        assert_eq!(cloned[key], original[key], "clone must preserve {key}");
    }
    let renamed = success_json(
        temp.path(),
        &[
            "update",
            "--profile-id",
            source_id,
            "--expected-revision",
            "0",
            "--name",
            "Source Changed",
        ],
    );
    assert_eq!(renamed["revision"], 1);
    let stale = run(
        temp.path(),
        &[
            "clone",
            "--profile-id",
            source_id,
            "--expected-revision",
            "0",
            "--name",
            "Stale Copy",
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("revision conflict"));
    let risky = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Risky",
            "--agent-type",
            "codex",
            "--launch-mode",
            "managed",
            "--managed-config",
            r#"{"approvalPolicy":"never"}"#,
            "--confirm-reduced-protections",
        ],
    );
    let risky_id = risky["id"].as_str().unwrap();
    let unconfirmed = run(
        temp.path(),
        &[
            "clone",
            "--profile-id",
            risky_id,
            "--expected-revision",
            "0",
            "--name",
            "Risky Copy",
        ],
    );
    assert!(!unconfirmed.status.success());
    assert!(String::from_utf8_lossy(&unconfirmed.stderr).contains("--confirm-reduced-protections"));
    let confirmed = success_json(
        temp.path(),
        &[
            "clone",
            "--profile-id",
            risky_id,
            "--expected-revision",
            "0",
            "--name",
            "Risky Copy",
            "--confirm-reduced-protections",
        ],
    );
    assert_ne!(confirmed["id"], risky["id"]);
    assert_eq!(confirmed["managedConfig"], risky["managedConfig"]);
    assert_eq!(confirmed["launchMode"], "managed");
    assert_eq!(confirmed["revision"], 0);
    let listed = success_json(temp.path(), &["list"]);
    assert_eq!(listed["items"].as_array().unwrap().len(), 4);
}

/// Native Routing editing must retain all untouched Host fields.
#[test]
fn agent_profile_routing_updates_only_requested_fields_and_guards_revision() {
    let temp = tempfile::tempdir().unwrap();
    let _guard = RuntimeGuard {
        runtime_dir: temp.path().to_path_buf(),
    };
    start_isolated_runtime(temp.path());
    let command = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Routing QA",
            "--agent-type",
            "codex",
            "--launch-mode",
            "command",
            "--command",
            "cmd.exe /Q",
            "--custom-prompt",
            "Keep project instructions",
            "--description",
            "Original Routing",
            "--quota-group",
            "qa-original",
        ],
    );
    let id = command["id"].as_str().unwrap();
    let shown = success_json(temp.path(), &["show", "--profile-id", id]);
    assert_eq!(shown["revision"], 0);
    assert_eq!(shown["description"], "Original Routing");
    assert_eq!(shown["quotaGroup"], "qa-original");
    let updated = success_json(
        temp.path(),
        &[
            "update",
            "--profile-id",
            id,
            "--expected-revision",
            "0",
            "--description",
            "After Native edit",
            "--quota-group",
            "qa-updated",
        ],
    );
    assert_eq!(updated["revision"], 1);
    assert_eq!(updated["description"], "After Native edit");
    assert_eq!(updated["quotaGroup"], "qa-updated");
    for key in [
        "id",
        "agentType",
        "launchMode",
        "command",
        "customPrompt",
        "managedConfig",
    ] {
        assert_eq!(updated[key], command[key], "Routing changed {key}");
    }
    let stale = run(
        temp.path(),
        &[
            "update",
            "--profile-id",
            id,
            "--expected-revision",
            "0",
            "--description",
            "Stale edit",
        ],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("revision conflict"));
    let cleared = success_json(
        temp.path(),
        &[
            "update",
            "--profile-id",
            id,
            "--expected-revision",
            "1",
            "--clear-description",
            "--clear-quota-group",
        ],
    );
    assert_eq!(cleared["revision"], 2);
    assert_eq!(cleared["description"], "");
    assert!(cleared["quotaGroup"].is_null());
    assert_eq!(cleared["command"], command["command"]);
    assert_eq!(cleared["customPrompt"], command["customPrompt"]);

    let managed = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Managed Routing QA",
            "--agent-type",
            "codex",
            "--launch-mode",
            "managed",
            "--managed-config",
            r#"{"approvalPolicy":"never","model":"test-model"}"#,
            "--custom-prompt",
            "Keep managed instructions",
            "--quota-group",
            "managed-old",
            "--confirm-reduced-protections",
        ],
    );
    let managed_id = managed["id"].as_str().unwrap();
    let managed_updated = success_json(
        temp.path(),
        &[
            "update",
            "--profile-id",
            managed_id,
            "--expected-revision",
            "0",
            "--description",
            "Managed routing only",
            "--quota-group",
            "managed-updated",
        ],
    );
    assert_eq!(managed_updated["revision"], 1);
    for key in [
        "id",
        "agentType",
        "launchMode",
        "command",
        "customPrompt",
        "managedConfig",
    ] {
        assert_eq!(
            managed_updated[key], managed[key],
            "Managed Routing changed {key}"
        );
    }
    assert_eq!(managed_updated["description"], "Managed routing only");
    assert_eq!(managed_updated["quotaGroup"], "managed-updated");
}

/// GPUI's Command/Prompt editor must write through stdin against a disposable
/// Host, preserve the other Host-owned fields, and reject stale revisions.
#[test]
fn agent_profile_stdin_command_prompt_roundtrip_on_isolated_host() {
    use std::io::Write;
    use std::process::Stdio;

    fn update_from_stdin(runtime_dir: &Path, id: &str, rev: i64, body: &str) -> Output {
        let mut child = windowless_command(env!("CARGO_BIN_EXE_alera"))
            .arg("agent-profile")
            .arg("--runtime-dir")
            .arg(runtime_dir)
            .arg("--json")
            .arg("--no-start")
            .args([
                "update",
                "--profile-id",
                id,
                "--expected-revision",
                &rev.to_string(),
                "--edit-content-stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("disposable CLI process");
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(body.as_bytes())
            .expect("send bounded edit body");
        child.wait_with_output().expect("CLI update result")
    }

    let temp = tempfile::tempdir().unwrap();
    let _guard = RuntimeGuard {
        runtime_dir: temp.path().to_path_buf(),
    };
    start_isolated_runtime(temp.path());
    let original = success_json(
        temp.path(),
        &[
            "create",
            "--name",
            "Prompt Roundtrip QA",
            "--agent-type",
            "codex",
            "--launch-mode",
            "command",
            "--command",
            "codex --original",
            "--custom-prompt",
            "Original instructions",
            "--description",
            "Description stays",
            "--quota-group",
            "group-original",
        ],
    );
    let id = original["id"].as_str().unwrap();
    let output = update_from_stdin(
        temp.path(),
        id,
        0,
        r#"{"command":"codex --revised","customPrompt":"First line\nSecond line"}"#,
    );
    assert!(
        output.status.success(),
        "stdin update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let saved: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["command"], "codex --revised");
    assert_eq!(saved["customPrompt"], "First line\nSecond line");
    for key in ["agentType", "launchMode", "description", "quotaGroup"] {
        assert_eq!(saved[key], original[key], "unexpected change to {key}");
    }

    let stale = update_from_stdin(
        temp.path(),
        id,
        0,
        r#"{"customPrompt":"Must not be saved"}"#,
    );
    assert!(!stale.status.success(), "stale write must fail");
    let after_stale = success_json(temp.path(), &["show", "--profile-id", id]);
    assert_eq!(after_stale["revision"], 1);
    assert_eq!(after_stale["customPrompt"], "First line\nSecond line");

    let cleared = update_from_stdin(temp.path(), id, 1, r#"{"clearCustomPrompt":true}"#);
    assert!(
        cleared.status.success(),
        "clear failed: {}",
        String::from_utf8_lossy(&cleared.stderr)
    );
    let after_clear = success_json(temp.path(), &["show", "--profile-id", id]);
    assert_eq!(after_clear["revision"], 2);
    assert_eq!(after_clear["customPrompt"], "");
    assert_eq!(after_clear["command"], "codex --revised");
    assert_eq!(after_clear["description"], "Description stays");
    assert_eq!(after_clear["quotaGroup"], "group-original");
}
