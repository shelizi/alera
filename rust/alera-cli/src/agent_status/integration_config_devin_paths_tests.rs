use super::*;

fn portable_fixture(home: &Path) -> (PathBuf, PathBuf) {
    let root = home.join("Devin Portable");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("devin.exe"), "").unwrap();
    std::fs::write(bin.join("_devin-env.cmd"), "@echo off\n").unwrap();
    let launcher = bin.join("devin.cmd");
    std::fs::write(&launcher, "@echo off\ncall \"%~dp0_devin-env.cmd\"\n").unwrap();
    let shims = home.join("scoop/shims");
    std::fs::create_dir_all(&shims).unwrap();
    std::fs::write(
        shims.join("devin.cmd"),
        format!(
            "@rem {}\r\n@\"{}\" %*\r\n",
            launcher.display(),
            launcher.display()
        ),
    )
    .unwrap();
    (root, shims)
}

#[test]
fn devin_portable_environment_selects_redirected_config() {
    let home = tempfile::tempdir().unwrap();
    let (root, _) = portable_fixture(home.path());
    let environment = BTreeMap::from([(
        "DEVIN_PORTABLE_ROOT".to_string(),
        root.to_string_lossy().into_owned(),
    )]);

    assert_eq!(
        portable_config_path(&environment),
        Some(root.join("data/home/AppData/Roaming/devin/config.json"))
    );
}

#[test]
fn devin_portable_scoop_shim_selects_redirected_config() {
    let home = tempfile::tempdir().unwrap();
    let (root, shims) = portable_fixture(home.path());
    let environment = BTreeMap::from([("PATH".to_string(), shims.to_string_lossy().into_owned())]);

    assert_eq!(
        portable_config_path(&environment),
        Some(root.join("data/home/AppData/Roaming/devin/config.json"))
    );
}

#[test]
fn devin_portable_batch_launcher_respects_pathext() {
    let home = tempfile::tempdir().unwrap();
    let (root, _) = portable_fixture(home.path());
    let mut environment = BTreeMap::from([(
        "PATH".to_string(),
        format!("\"{}\"", root.join("bin").display()),
    )]);

    assert_eq!(portable_config_path(&environment), None);
    environment.insert("PATHEXT".to_string(), ".CMD;.EXE".to_string());
    assert_eq!(
        portable_config_path(&environment),
        Some(root.join("data/home/AppData/Roaming/devin/config.json"))
    );
}

#[test]
fn devin_portable_discovery_does_not_override_first_standard_executable() {
    let home = tempfile::tempdir().unwrap();
    let (_, shims) = portable_fixture(home.path());
    let bin = home.path().join("standard-bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("devin.exe"), "").unwrap();
    let environment = BTreeMap::from([(
        "PATH".to_string(),
        format!("{};{}", bin.display(), shims.display()),
    )]);

    assert_eq!(portable_config_path(&environment), None);
}

#[test]
fn devin_portable_discovery_rejects_incomplete_installations() {
    let home = tempfile::tempdir().unwrap();
    let environment = BTreeMap::from([(
        "DEVIN_PORTABLE_ROOT".to_string(),
        home.path().to_string_lossy().into_owned(),
    )]);

    assert_eq!(portable_config_path(&environment), None);
}

#[test]
fn devin_portable_discovery_ignores_broken_shims() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("devin.cmd"), "@rem missing-devin.cmd\n").unwrap();
    let environment = BTreeMap::from([(
        "PATH".to_string(),
        home.path().to_string_lossy().into_owned(),
    )]);

    assert_eq!(portable_config_path(&environment), None);
}
