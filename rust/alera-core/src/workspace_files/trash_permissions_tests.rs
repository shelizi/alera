use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::{open_workspace_text_trash, STORAGE_DIR};
use crate::workspace_files::open_workspace_file_root;

#[test]
fn trash_directories_are_private_including_existing_storage() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let storage = runtime.path().join(STORAGE_DIR);
    fs::create_dir(&storage).unwrap();
    fs::set_permissions(&storage, fs::Permissions::from_mode(0o755)).unwrap();
    let root = open_workspace_file_root(&workspace.path().to_string_lossy()).unwrap();
    let trash = open_workspace_text_trash(runtime.path(), "workspace-A", &root).unwrap();

    assert_eq!(
        fs::metadata(&storage).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let metadata = trash
        .dir
        .try_clone()
        .unwrap()
        .into_std_file()
        .metadata()
        .unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
}
