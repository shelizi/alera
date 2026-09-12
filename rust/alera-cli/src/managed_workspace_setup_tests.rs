use std::path::Path;
use std::process::Command as StdCommand;

use alera_core::runtime::{Project, ProjectKind, RuntimeStore, WorktreeSetupStepKind};
use chrono::Utc;

use crate::managed_workspace::{create_managed_workspace, ManagedWorkspaceCreateRequest};

#[tokio::test]
async fn deferred_setup_writes_a_script_instead_of_running_the_commands() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    std::fs::write(repo.join(".env"), "TOKEN=1\n").unwrap();
    std::fs::write(
        repo.join("alera.toml"),
        "[worktree]\ncopy = [{ from = \".env\" }]\nsetup = [\"pnpm install\", \"pnpm build\"]\n",
    )
    .unwrap();
    let store = seed_project(dir.path(), &repo).await;
    let scripts = dir.path().join("scripts");
    let worktree_path = dir.path().join("workspaces").join("feature-deferred");

    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-deferred".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/deferred".to_string()),
            branch: "feature/deferred".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(scripts.clone()),
        },
    )
    .await
    .unwrap();

    // Nothing ran: no copy landed and the report carries no steps, so the
    // create dialog has nothing to wait on.
    assert!(result.setup_report.steps.is_empty());
    assert!(!worktree_path.join(".env").exists());

    let command = result.deferred_setup_command.expect("deferred command");
    let script = crate::worktree_setup_script::setup_script_path(
        &scripts,
        "workspace-deferred",
        cfg!(windows),
    );
    assert!(script.exists(), "{}", script.display());
    assert!(command.contains(&script.display().to_string()), "{command}");
    let contents = std::fs::read_to_string(&script).unwrap();
    assert!(contents.contains("pnpm install"), "{contents}");
    assert!(contents.contains("pnpm build"), "{contents}");
    assert!(contents.contains("--copies-only"), "{contents}");
    assert!(!contents.contains("&&"), "{contents}");
}

#[tokio::test]
async fn deferred_setup_stays_quiet_when_the_project_configures_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let store = seed_project(dir.path(), &repo).await;
    let scripts = dir.path().join("scripts");
    let worktree_path = dir.path().join("workspaces").join("feature-plain");

    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-plain".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/plain".to_string()),
            branch: "feature/plain".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(scripts.clone()),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.deferred_setup_command, None);
    assert!(result.setup_report.steps.is_empty());
    assert!(!scripts.exists());
}

#[tokio::test]
async fn deferred_setup_reports_an_invalid_config_like_the_inline_path() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    std::fs::write(repo.join("alera.toml"), "[worktree]\nsetup = 3\n").unwrap();
    let store = seed_project(dir.path(), &repo).await;
    let worktree_path = dir.path().join("workspaces").join("feature-broken");

    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-broken".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/broken".to_string()),
            branch: "feature/broken".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(dir.path().join("scripts")),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.deferred_setup_command, None);
    let step = result.setup_report.steps.first().expect("config step");
    assert_eq!(step.kind, WorktreeSetupStepKind::Config);
    assert!(!step.succeeded);
}

#[tokio::test]
async fn setup_copies_only_applies_the_copy_rules_and_keeps_going_after_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    std::fs::write(repo.join(".env"), "TOKEN=1\n").unwrap();
    std::fs::write(
            repo.join("alera.toml"),
            "[worktree]\ncopy = [{ from = \"missing.env\" }, { from = \".env\" }]\nsetup = [\"exit 7\"]\n",
        )
        .unwrap();
    let store = seed_project(dir.path(), &repo).await;
    let worktree_path = dir.path().join("workspaces").join("feature-copies");
    create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-copies".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/copies".to_string()),
            branch: "feature/copies".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(dir.path().join("scripts")),
        },
    )
    .await
    .unwrap();

    let report = crate::worktree_setup::run_workspace_setup(&store, "workspace-copies", true)
        .await
        .unwrap();

    // The first rule fails and the second still runs, matching what the
    // script does with the commands.
    assert_eq!(report.steps.len(), 2);
    assert!(!report.steps[0].succeeded);
    assert!(report.steps[1].succeeded);
    assert!(report
        .steps
        .iter()
        .all(|step| step.kind == WorktreeSetupStepKind::Copy));
    assert!(worktree_path.join(".env").exists());
}

#[tokio::test]
async fn setup_copies_only_applies_worktreeinclude_matches() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    std::fs::write(repo.join(".gitignore"), ".env.local\n").unwrap();
    std::fs::write(repo.join(".worktreeinclude"), ".env.local\n").unwrap();
    run_git(&repo, &["add", ".gitignore", ".worktreeinclude"]);
    run_git(&repo, &["commit", "-m", "include"]);
    std::fs::write(repo.join(".env.local"), "TOKEN=1\n").unwrap();
    let store = seed_project(dir.path(), &repo).await;
    let worktree_path = dir.path().join("workspaces").join("feature-include");
    create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-include".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/include".to_string()),
            branch: "feature/include".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(dir.path().join("scripts")),
        },
    )
    .await
    .unwrap();

    let report = crate::worktree_setup::run_workspace_setup(&store, "workspace-include", true)
        .await
        .unwrap();

    assert_eq!(report.steps.len(), 1);
    assert!(report.steps[0].succeeded);
    assert_eq!(report.steps[0].kind, WorktreeSetupStepKind::Copy);
    assert_eq!(
        std::fs::read_to_string(worktree_path.join(".env.local")).unwrap(),
        "TOKEN=1\n"
    );
}

#[tokio::test]
async fn deferred_setup_runs_copies_when_only_worktreeinclude_exists() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    std::fs::write(repo.join(".worktreeinclude"), ".env.local\n").unwrap();
    run_git(&repo, &["add", ".worktreeinclude"]);
    run_git(&repo, &["commit", "-m", "include"]);
    let store = seed_project(dir.path(), &repo).await;
    let scripts = dir.path().join("scripts");
    let worktree_path = dir.path().join("workspaces").join("feature-include-only");
    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-include-only".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/include-only".to_string()),
            branch: "feature/include-only".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: false,
            setup_script_directory: Some(scripts.clone()),
        },
    )
    .await
    .unwrap();

    let command = result.deferred_setup_command.expect("setup command");
    let script = crate::worktree_setup_script::setup_script_path(
        &scripts,
        "workspace-include-only",
        cfg!(windows),
    );
    assert!(script.exists(), "{}", script.display());
    assert!(command.contains(&script.display().to_string()), "{command}");
    let contents = std::fs::read_to_string(&script).unwrap();
    assert!(contents.contains("--copies-only"), "{contents}");
}

async fn seed_project(root: &Path, repo: &Path) -> RuntimeStore {
    let store = RuntimeStore::open(&root.join("runtime")).await.unwrap();
    let now = Utc::now();
    store
        .upsert_project(Project {
            id: "project-1".to_string(),
            name: "Project".to_string(),
            repo_path: repo.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: ProjectKind::GitRepository,
        })
        .await
        .unwrap();
    store
}

fn init_git_repo(repo: &Path) {
    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "test@example.com"]);
    run_git(repo, &["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-m", "initial"]);
    run_git(repo, &["branch", "-M", "main"]);
}

// Test fixture: it runs from a console, so the console-window suppression in
// `alera_core::child_process` does not apply.
#[allow(clippy::disallowed_methods)]
fn run_git(repo: &Path, args: &[&str]) {
    let output = StdCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
