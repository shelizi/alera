use super::*;

#[test]
fn clone_destination_rejects_nested_names() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_clone_destination(dir.path().to_str().unwrap(), "nested/repo").is_err());
    assert!(validate_clone_destination(dir.path().to_str().unwrap(), "repo").is_ok());
}

#[test]
fn cleanup_never_removes_outside_parent() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    assert!(remove_owned_clone_destination(
        parent.path().to_str().unwrap(),
        outside.path().to_str().unwrap(),
    )
    .is_err());
    assert!(outside.path().exists());
}

#[tokio::test]
async fn registering_a_folder_creates_one_main_workspace_and_deduplicates() {
    let runtime = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(runtime.path()).await.unwrap();

    let first = register_project(&store, project_dir.path().to_str().unwrap(), Some("Sample"))
        .await
        .unwrap();
    let second = register_project(
        &store,
        project_dir.path().to_str().unwrap(),
        Some("Ignored"),
    )
    .await
    .unwrap();

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.project.id, second.project.id);
    assert_eq!(first.main_workspace.id, second.main_workspace.id);
    assert_eq!(first.project.kind, ProjectKind::Folder);
    assert_eq!(first.main_workspace.name, "Sample");
    assert_eq!(store.list_projects().await.unwrap().len(), 1);
    assert_eq!(
        store
            .list_workspaces(&first.project.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn registering_a_git_project_names_the_main_workspace_after_its_branch() {
    let runtime = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    run_git(project_dir.path(), &["init"]);
    run_git(
        project_dir.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(project_dir.path(), &["config", "user.name", "Test"]);
    std::fs::write(project_dir.path().join("README.md"), "hello\n").unwrap();
    run_git(project_dir.path(), &["add", "README.md"]);
    run_git(project_dir.path(), &["commit", "-m", "initial"]);
    run_git(project_dir.path(), &["branch", "-M", "trunk"]);
    let store = RuntimeStore::open(runtime.path()).await.unwrap();

    let registered = register_project(&store, project_dir.path().to_str().unwrap(), Some("Sample"))
        .await
        .unwrap();

    assert_eq!(registered.project.name, "Sample");
    assert_eq!(registered.main_workspace.branch.as_deref(), Some("trunk"));
    assert_eq!(registered.main_workspace.name, "trunk");
}

#[tokio::test]
async fn effective_config_prefers_runtime_override() {
    let runtime = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(runtime.path()).await.unwrap();
    let registered = register_project(&store, project_dir.path().to_str().unwrap(), Some("Sample"))
        .await
        .unwrap();
    let config = ProjectConfig {
        git_hosting_provider: Some("github".to_string()),
        ..ProjectConfig::default()
    };
    store
        .upsert_project_config(&registered.project.id, config.clone(), Utc::now())
        .await
        .unwrap();

    let effective = effective_project_config(&store, &registered.project.id)
        .await
        .unwrap();
    assert_eq!(effective.origin, "uiOverride");
    assert_eq!(effective.config, config);
    assert!(effective.error.is_none());
}

#[allow(clippy::disallowed_methods)]
fn run_git(directory: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
