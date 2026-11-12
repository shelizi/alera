use super::{Project, ProjectKind, RuntimeStore, Workspace, WorkspaceKind, WorkspaceStatus};
use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::Path;

fn project(id: &str, path: &Path) -> Project {
    let now = Utc::now();
    Project {
        id: id.into(),
        name: id.into(),
        repo_path: dunce::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .to_string(),
        created_at: now,
        updated_at: now,
        kind: ProjectKind::GitRepository,
    }
}

fn linked_workspace(id: &str, project_id: &str, path: &Path) -> Workspace {
    let now = Utc::now();
    Workspace {
        id: id.into(),
        instance_id: format!("{id}-instance"),
        host_id: "local".into(),
        project_id: project_id.into(),
        name: id.into(),
        branch: Some("feature/test".into()),
        path: dunce::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .to_string(),
        created_at: now,
        updated_at: now,
        kind: WorkspaceKind::Linked,
        status: WorkspaceStatus::Active,
        source_branch: None,
        reuses_existing_branch: false,
        is_pinned: false,
        tag_ids: Vec::new(),
        tag_names: Vec::new(),
        section_id: None,
        parent_workspace_id: None,
        child_count: 0,
        archived_at: None,
    }
}

fn list(branches: &[&str]) -> Vec<String> {
    branches.iter().map(|name| (*name).to_owned()).collect()
}

#[tokio::test]
async fn classic_and_native_share_project_identity_across_worktrees_and_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("main");
    let linked = temp.path().join("linked");
    let rogue = temp.path().join("rogue");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&linked).unwrap();
    fs::create_dir_all(&rogue).unwrap();
    let store_path = temp.path().join("runtime");
    let classic = RuntimeStore::open(&store_path).await.unwrap();
    let native = RuntimeStore::open(&store_path).await.unwrap();
    classic
        .upsert_project(project("project-one", &root))
        .await
        .unwrap();
    classic
        .upsert_workspace(linked_workspace("wt-1", "project-one", &linked))
        .await
        .unwrap();

    let main_key = json!(["project-one", null]).to_string();
    let nested_key = json!(["project-one", "packages/subrepo"]).to_string();
    let seeds = classic
        .git_favorites_seed_classic(&json!({
            main_key.clone(): ["main", "feature/classic"],
            nested_key.clone(): ["release/nested"]
        }))
        .await
        .unwrap();
    assert_eq!(seeds["changed"], true);
    assert_eq!(seeds["skipped"].as_array().unwrap().len(), 0);

    let linked_path = dunce::canonicalize(&linked)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let from_native = native
        .git_favorites_for_workspace(&linked_path)
        .await
        .unwrap();
    assert_eq!(from_native["scopeKey"], main_key);
    assert_eq!(from_native["branches"], json!(["main", "feature/classic"]));
    assert_eq!(from_native["present"], true);

    let updated = native
        .git_favorites_cas_for_workspace(
            &linked_path,
            &list(&["main", "feature/classic"]),
            &list(&["release/native"]),
        )
        .await
        .unwrap();
    assert_eq!(updated["changed"], true);
    assert!(classic
        .git_favorites_compare_and_set(
            &main_key,
            &list(&["main", "feature/classic"]),
            &list(&["stale"]),
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("git_favorites_revision_conflict"));
    assert_eq!(
        classic.git_favorites_snapshot().await.unwrap()["scopes"][&nested_key],
        json!(["release/nested"])
    );

    let reopened = RuntimeStore::open(&store_path).await.unwrap();
    assert_eq!(
        reopened
            .git_favorites_for_workspace(&linked_path)
            .await
            .unwrap()["branches"],
        json!(["release/native"])
    );
    let main_root = dunce::canonicalize(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_eq!(
        reopened
            .git_favorites_for_workspace(&main_root)
            .await
            .unwrap()["branches"],
        json!(["release/native"])
    );
    let rogue_root = dunce::canonicalize(&rogue)
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(reopened
        .git_favorites_for_workspace(&rogue_root)
        .await
        .is_err());
}

#[tokio::test]
async fn native_import_is_insert_only_and_cleared_scope_has_tombstone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let store = RuntimeStore::open(&temp.path().join("store"))
        .await
        .unwrap();
    store.upsert_project(project("p", &root)).await.unwrap();
    let root = dunce::canonicalize(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let key = json!(["p", null]).to_string();

    let seeded = store
        .git_favorites_seed_native(&root, &list(&["old/native"]))
        .await
        .unwrap();
    assert_eq!(seeded["changed"], true);
    assert_eq!(
        store
            .git_favorites_seed_classic(&json!({key.clone(): ["classic/late"]}))
            .await
            .unwrap()["changed"],
        false
    );
    let cleared = store
        .git_favorites_cas_for_workspace(&root, &list(&["old/native"]), &[])
        .await
        .unwrap();
    assert_eq!(cleared["branches"], json!([]));
    assert_eq!(
        store
            .git_favorites_seed_native(&root, &list(&["should/not/reappear"]))
            .await
            .unwrap()["changed"],
        false
    );
    assert_eq!(
        store.git_favorites_for_workspace(&root).await.unwrap()["branches"],
        json!([])
    );
    assert_eq!(
        store.git_favorites_snapshot().await.unwrap()["scopes"][&key],
        json!([])
    );
}

#[tokio::test]
async fn folder_project_can_pin_nested_git_root_but_not_folder_root() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("collection");
    fs::create_dir_all(&folder).unwrap();
    let store = RuntimeStore::open(&temp.path().join("store"))
        .await
        .unwrap();
    let mut record = project("folder-project", &folder);
    record.kind = ProjectKind::Folder;
    store.upsert_project(record).await.unwrap();
    let nested = json!(["folder-project", "nested/repo"]).to_string();
    let folder_root = json!(["folder-project", null]).to_string();
    store
        .git_favorites_seed_classic(&json!({
            nested.clone(): ["main"],
            folder_root.clone(): ["should/not/persist"],
        }))
        .await
        .unwrap();
    assert_eq!(
        store.git_favorites_snapshot().await.unwrap()["scopes"][&nested],
        json!(["main"])
    );
    assert!(store
        .git_favorites_compare_and_set(&folder_root, &[], &list(&["invalid"]))
        .await
        .is_err());
    store
        .git_favorites_compare_and_set(&nested, &list(&["main"]), &list(&["feature/folder"]))
        .await
        .unwrap();
    assert_eq!(
        store.git_favorites_snapshot().await.unwrap()["scopes"][&nested],
        json!(["feature/folder"])
    );
}

#[tokio::test]
async fn project_identity_validation_prevents_cross_project_or_nested_scope_leaks() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    let store = RuntimeStore::open(&temp.path().join("store"))
        .await
        .unwrap();
    store
        .upsert_project(project("project-a", &a))
        .await
        .unwrap();
    store
        .upsert_project(project("project-b", &b))
        .await
        .unwrap();
    let scope_a = json!(["project-a", null]).to_string();
    let scope_b = json!(["project-b", null]).to_string();
    let nested = json!(["project-a", "vendor/subrepo"]).to_string();
    store
        .git_favorites_seed_classic(&json!({
            scope_a.clone(): ["a-main"],
            scope_b.clone(): ["b-main"],
            nested.clone(): ["nested-main"],
            "[\"removed-project\",null]": ["orphan"]
        }))
        .await
        .unwrap();
    let snapshot = store.git_favorites_snapshot().await.unwrap();
    assert_eq!(snapshot["scopes"][&scope_a], json!(["a-main"]));
    assert_eq!(snapshot["scopes"][&scope_b], json!(["b-main"]));
    assert_eq!(snapshot["scopes"][&nested], json!(["nested-main"]));
    assert!(snapshot["scopes"]["[\"removed-project\",null]"].is_null());
    for bad_key in [
        "[\"project-a\",\"../secret\"]",
        "[\"project-a\",\"vendor/../elsewhere\"]",
        "[\"project-a\",\"\\\\\\\\unsafe\"]",
        "[\"project-a\",null,123]",
    ] {
        assert!(store
            .git_favorites_compare_and_set(bad_key, &[], &list(&["unsafe"]))
            .await
            .is_err());
    }
    assert!(store
        .git_favorites_compare_and_set(&scope_a, &list(&["a-main"]), &list(&["dup", "dup"]))
        .await
        .is_err());
    // Two distinct Host Projects pointing to the same canonical path are
    // intentionally ambiguous: do not choose whichever happens to sort first.
    store
        .upsert_project(project("duplicate", &a))
        .await
        .unwrap();
    assert!(store
        .git_favorites_for_workspace(&dunce::canonicalize(&a).unwrap().to_string_lossy())
        .await
        .is_err());
}
