use std::path::Path;
use std::process::Command as StdCommand;

use alera_core::git as core_git;
use alera_core::runtime::{
    Project, ProjectKind, RuntimeStore, Workspace, WorkspaceKind, WorkspaceStatus, LOCAL_HOST_ID,
};
use chrono::Utc;

use crate::managed_workspace::{
    create_managed_workspace, slugify, switch_managed_workspace_branch,
    ManagedWorkspaceCreateRequest, ManagedWorkspaceSwitchBranchRequest,
};
#[test]
fn slugify_matches_workspace_path_segments() {
    assert_eq!(slugify("Feature/Coverage").unwrap(), "feature-coverage");
    assert_eq!(slugify("  Fix UI  State  ").unwrap(), "fix-ui-state");
    assert!(slugify("///").is_err());
}

#[tokio::test]
async fn create_managed_workspace_rejects_existing_id_before_worktree_create() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);

    let store = RuntimeStore::open(&dir.path().join("runtime"))
        .await
        .unwrap();
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
        .upsert_workspace(Workspace {
            id: "workspace-1".to_string(),
            instance_id: "instance-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            project_id: "project-1".to_string(),
            name: "Main".to_string(),
            branch: Some("main".to_string()),
            path: repo.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: WorkspaceKind::Main,
            status: WorkspaceStatus::Active,
            source_branch: None,
            reuses_existing_branch: false,
            is_pinned: false,
            tag_ids: Vec::new(),
            tag_names: Vec::new(),
            parent_workspace_id: None,
            section_id: None,
            child_count: 0,
            archived_at: None,
        })
        .await
        .unwrap();

    let worktree_path = dir.path().join("workspaces").join("feature-collide");
    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-1".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/collide".to_string()),
            branch: "feature/collide".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: false,
            skip_setup: false,
            setup_script_directory: None,
        },
    )
    .await;

    let error = result.unwrap_err().to_string();
    assert!(error.contains("workspace-1"));
    assert!(!worktree_path.exists());
    let existing = store.find_workspace("workspace-1").await.unwrap().unwrap();
    assert_eq!(existing.kind, WorkspaceKind::Main);
    assert_eq!(existing.path, repo.to_string_lossy());
}

#[tokio::test]
async fn create_managed_workspace_rejects_missing_parent_before_worktree_create() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let store = RuntimeStore::open(&dir.path().join("runtime"))
        .await
        .unwrap();
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
    let worktree_path = dir.path().join("workspaces").join("feature-child");

    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-child".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/child".to_string()),
            branch: "feature/child".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: Some("missing-parent".to_string()),
            defer_setup: false,
            skip_setup: false,
            setup_script_directory: None,
        },
    )
    .await;

    assert!(result.unwrap_err().to_string().contains("missing-parent"));
    assert!(!worktree_path.exists());
    assert!(store
        .find_workspace("workspace-child")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn create_managed_workspace_returns_the_linked_parent() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let store = RuntimeStore::open(&dir.path().join("runtime"))
        .await
        .unwrap();
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
        .upsert_workspace(Workspace {
            id: "parent".to_string(),
            instance_id: "parent-instance".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            project_id: "project-1".to_string(),
            name: "Parent".to_string(),
            branch: Some("main".to_string()),
            path: repo.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: WorkspaceKind::Main,
            status: WorkspaceStatus::Active,
            source_branch: None,
            reuses_existing_branch: false,
            is_pinned: false,
            tag_ids: Vec::new(),
            tag_names: Vec::new(),
            parent_workspace_id: None,
            section_id: None,
            child_count: 0,
            archived_at: None,
        })
        .await
        .unwrap();
    let worktree_path = dir.path().join("workspaces").join("feature-child");

    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("child".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/child".to_string()),
            branch: "feature/child".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: Some("parent".to_string()),
            defer_setup: false,
            skip_setup: false,
            setup_script_directory: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        result.workspace.parent_workspace_id.as_deref(),
        Some("parent")
    );
}

#[tokio::test]

async fn create_managed_workspace_succeeds_when_source_branch_refresh_fails() {
    let dir = tempfile::tempdir().unwrap();
    let remote_bare = dir.path().join("bare.git");
    run_git(dir.path(), &["init", "--bare", "bare.git"]);

    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    run_git(
        &repo,
        &["remote", "add", "origin", remote_bare.to_str().unwrap()],
    );
    run_git(&repo, &["push", "-u", "origin", "main"]);
    run_git(&remote_bare, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let other = dir.path().join("other");
    run_git(
        dir.path(),
        &[
            "clone",
            "-b",
            "main",
            remote_bare.to_str().unwrap(),
            "other",
        ],
    );
    run_git(&other, &["config", "user.email", "other@example.com"]);
    run_git(&other, &["config", "user.name", "Other"]);
    std::fs::write(other.join("remote.txt"), "from remote\n").unwrap();
    run_git(&other, &["add", "remote.txt"]);
    run_git(&other, &["commit", "-m", "remote commit"]);
    run_git(&other, &["push", "origin", "main"]);

    std::fs::write(repo.join("local.txt"), "from local\n").unwrap();
    run_git(&repo, &["add", "local.txt"]);
    run_git(&repo, &["commit", "-m", "local commit"]);

    let store = seed_project(dir.path(), &repo).await;
    let worktree_path = dir.path().join("workspaces").join("feature-diverged");
    let result = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-diverged".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/diverged".to_string()),
            branch: "feature/diverged".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: true,
            setup_script_directory: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(result.workspace.branch.as_deref(), Some("feature/diverged"));
    assert!(worktree_path.join("local.txt").exists());
}

#[tokio::test]
async fn switch_managed_workspace_branch_updates_checkout_and_runtime_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    run_git(&repo, &["branch", "feature/existing"]);
    let store = seed_project(dir.path(), &repo).await;
    let worktree_path = dir.path().join("workspaces").join("feature-current");
    let created = create_managed_workspace(
        &store,
        ManagedWorkspaceCreateRequest {
            id: Some("workspace-switch".to_string()),
            project_id: "project-1".to_string(),
            name: Some("feature/current".to_string()),
            branch: "feature/current".to_string(),
            source_branch: Some("main".to_string()),
            reuse_existing_branch: false,
            workspace_root: None,
            path: Some(worktree_path.to_string_lossy().into_owned()),
            parent_workspace_id: None,
            defer_setup: true,
            skip_setup: true,
            setup_script_directory: None,
        },
    )
    .await
    .unwrap();
    assert!(!created.workspace.reuses_existing_branch);

    let switched = switch_managed_workspace_branch(
        &store,
        ManagedWorkspaceSwitchBranchRequest {
            id: created.workspace.id.clone(),
            branch: "feature/existing".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(switched.branch.as_deref(), Some("feature/existing"));
    assert!(switched.source_branch.is_none());
    assert!(switched.reuses_existing_branch);
    assert_eq!(
        core_git::current_branch(&switched.path).unwrap(),
        "feature/existing"
    );
    let persisted = store
        .find_workspace(&switched.id)
        .await
        .unwrap()
        .expect("persisted workspace");
    assert_eq!(persisted.branch.as_deref(), Some("feature/existing"));
    assert!(persisted.reuses_existing_branch);
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
