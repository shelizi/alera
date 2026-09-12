use super::*;

#[test]
fn split_clone_destination_uses_basename_under_parent() {
    assert_eq!(
        split_clone_destination("repos/demo").unwrap(),
        ("repos".to_string(), "demo".to_string())
    );
    assert_eq!(
        split_clone_destination("/abs/repos/demo").unwrap(),
        ("/abs/repos".to_string(), "demo".to_string())
    );
    assert_eq!(
        split_clone_destination("demo").unwrap(),
        (".".to_string(), "demo".to_string())
    );
}

#[test]
fn clones_from_local_source_into_nested_destination() {
    let source = init_repo();
    let workspace = tempfile::tempdir().expect("tempdir");
    let destination = workspace.path().join("nested").join("cloned");
    std::fs::create_dir_all(destination.parent().unwrap()).expect("create parent");

    clone_repository(path_str(source.path()), path_str(&destination)).unwrap();

    assert!(destination.join(".git").exists());
    assert!(!destination.join("cloned").exists());
    assert!(is_git_repository(path_str(&destination)).unwrap());
}
