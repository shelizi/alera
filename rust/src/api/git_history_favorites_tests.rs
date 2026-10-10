use super::*;

#[test]
fn favorite_history_walks_multiple_tips_without_unselected_head_and_pages() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature-a"]);
    commit_file(repo.path(), "a.txt", "a", "favorite a");
    run_git(repo.path(), &["checkout", "main"]);
    run_git(repo.path(), &["checkout", "-b", "feature-b"]);
    commit_file(repo.path(), "b.txt", "b", "favorite b");
    run_git(repo.path(), &["checkout", "main"]);
    commit_file(repo.path(), "main.txt", "main", "unselected main");
    let refs = vec!["feature-a".into(), "feature-b".into()];
    let full =
        git_history_for_refs(path_str(repo.path()), refs.clone(), Some(50), None, None).unwrap();
    assert_eq!(full.items.len(), 3);
    assert!(full
        .items
        .iter()
        .all(|item| item.subject != "unselected main"));
    assert!(full.current_ref.is_none());
    let first =
        git_history_for_refs(path_str(repo.path()), refs.clone(), Some(1), None, None).unwrap();
    let rest = git_history_for_refs(path_str(repo.path()), refs, Some(50), Some(1), None).unwrap();
    assert!(first.has_more);
    let paged: Vec<_> = first
        .items
        .iter()
        .chain(rest.items.iter())
        .map(|item| &item.id)
        .collect();
    assert_eq!(
        paged,
        full.items.iter().map(|item| &item.id).collect::<Vec<_>>()
    );
    assert_eq!(
        head_branch_name(&git2::Repository::open(repo.path()).unwrap()),
        "main"
    );
    let empty = git_history_for_refs(path_str(repo.path()), vec![], None, None, None).unwrap();
    assert!(empty.items.is_empty());
    assert!(!empty.has_more);
}

#[test]
fn favorite_history_keeps_ancestry_and_only_selected_branch_badges() {
    let repo = init_repo();
    commit_file(repo.path(), "target.txt", "root", "file root");
    run_git(repo.path(), &["checkout", "-b", "favorite"]);
    commit_file(repo.path(), "target.txt", "favorite", "favorite file");
    run_git(repo.path(), &["branch", "alias"]);
    commit_file(repo.path(), "other.txt", "other", "unrelated favorite tip");
    run_git(repo.path(), &["checkout", "main"]);
    commit_file(repo.path(), "target.txt", "main", "unselected file");
    let selected = git_history_for_refs(
        path_str(repo.path()),
        vec!["favorite".into()],
        Some(50),
        None,
        Some("target.txt".into()),
    )
    .unwrap();
    assert_eq!(selected.items.len(), 2);
    assert_eq!(selected.items[0].subject, "favorite file");
    assert_eq!(
        selected.items[0].parent_ids,
        vec![selected.items[1].id.clone()]
    );
    assert!(selected.items[0]
        .references
        .iter()
        .any(|reference| reference.name == "favorite"));
    assert!(selected
        .items
        .iter()
        .flat_map(|item| &item.references)
        .all(|reference| reference.name != "alias" && reference.name != "main"));
}
