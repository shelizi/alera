use std::fs;

use super::*;
use tempfile::tempdir;

#[test]
fn search_can_include_git_ignored_files() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(dir.path().join("visible.txt"), "needle\n").unwrap();
    fs::write(dir.path().join("ignored.txt"), "needle\n").unwrap();
    fs::write(dir.path().join(".git/config"), "needle\n").unwrap();

    let hidden = search_workspace(search_options(dir.path(), false)).unwrap();

    assert_eq!(hidden.total_matches, 1);
    assert_eq!(hidden.files.len(), 1);
    assert_eq!(hidden.files[0].relative_path, "visible.txt");

    let included = search_workspace(search_options(dir.path(), true)).unwrap();
    let paths = included
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(included.total_matches, 2);
    assert!(paths.contains(&"visible.txt"));
    assert!(paths.contains(&"ignored.txt"));
    assert!(!paths.contains(&".git/config"));
}

#[test]
fn full_text_search_stays_in_current_worktree_even_with_ignored_files() {
    let workspace = tempdir().unwrap();
    let root = workspace.path();
    let repo = git2::Repository::init(root).unwrap();
    fs::write(root.join(".gitignore"), ".worktrees/\n").unwrap();
    fs::write(root.join("local.txt"), "needle\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(".gitignore")).unwrap();
    index.add_path(std::path::Path::new("local.txt")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let signature = git2::Signature::now("Alera tests", "tests@alera.build").unwrap();
    let head_oid = repo
        .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    drop(tree);

    let head = repo.find_commit(head_oid).unwrap();
    let reference = repo
        .branch("feature/child", &head, false)
        .unwrap()
        .into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    fs::create_dir(root.join(".worktrees")).unwrap();
    let child = root.join(".worktrees/child");
    repo.worktree("child", &child, Some(&options)).unwrap();
    fs::write(child.join("hidden.txt"), "needle\n").unwrap();
    fs::create_dir(root.join(".worktrees/ordinary")).unwrap();
    fs::write(root.join(".worktrees/ordinary/file.txt"), "needle\n").unwrap();

    for include_ignored in [false, true] {
        let result = search_workspace(search_options(root, include_ignored)).unwrap();
        let paths = result
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"local.txt"));
        assert!(!paths
            .iter()
            .any(|path| path.starts_with(".worktrees/child")));
        assert_eq!(
            paths.contains(&".worktrees/ordinary/file.txt"),
            include_ignored
        );
    }

    // The linked child itself remains searchable when selected as workspace.
    let selected_child = search_workspace(search_options(&child, false)).unwrap();
    assert!(selected_child
        .files
        .iter()
        .any(|file| file.relative_path == "hidden.txt"));
}

fn search_options(path: &std::path::Path, include_ignored: bool) -> WorkspaceSearchOptions {
    WorkspaceSearchOptions {
        workspace_path: path.to_string_lossy().to_string(),
        query: "needle".to_string(),
        case_sensitive: true,
        whole_word: false,
        use_regex: false,
        include_pattern: None,
        exclude_pattern: None,
        include_ignored,
        max_results: None,
    }
}
