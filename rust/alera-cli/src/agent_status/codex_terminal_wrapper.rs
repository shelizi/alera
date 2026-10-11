use std::collections::BTreeMap;
#[cfg(windows)]
use std::hash::{Hash, Hasher};
use std::path::Path;

pub(super) fn prepare_codex_terminal_wrapper(
    runtime_dir: &Path,
    environment: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    #[cfg(windows)]
    let directory = native_wrapper_directory(runtime_dir)?;
    #[cfg(not(windows))]
    let directory = runtime_dir.join("agent-command-wrappers/codex");
    std::fs::create_dir_all(&directory)?;
    for (name, source) in wrapper_scripts(cfg!(windows)) {
        let path = directory.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(source) {
            std::fs::write(&path, source)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    environment.insert(
        "ALERA_CODEX_WRAPPER_PATH".into(),
        directory.to_string_lossy().into_owned(),
    );
    let mut directories = vec![directory];
    if let Some(previous) = environment.get("ALERA_AGENT_WRAPPER_PATH") {
        directories.extend(
            std::env::split_paths(previous)
                .filter(|entry| entry != &directories[0])
                .collect::<Vec<_>>(),
        );
    }
    environment.insert(
        "ALERA_AGENT_WRAPPER_PATH".into(),
        std::env::join_paths(directories)?
            .to_string_lossy()
            .into_owned(),
    );
    environment.insert(
        "ALERA_CODEX_LAUNCHER".into(),
        std::env::current_exe()?.to_string_lossy().into_owned(),
    );
    Ok(())
}

#[cfg(windows)]
fn native_wrapper_directory(runtime_dir: &Path) -> anyhow::Result<std::path::PathBuf> {
    let executable = std::env::current_exe()?;
    let metadata = executable.metadata()?;
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    executable.hash(&mut fingerprint);
    metadata.len().hash(&mut fingerprint);
    metadata.modified()?.hash(&mut fingerprint);
    // Keep each sidecar build separate so a restart never overwrites a
    // launcher still executing for an older terminal.
    let directory = runtime_dir
        .join("agent-command-wrappers/codex")
        .join(format!("{:016x}", fingerprint.finish()));
    std::fs::create_dir_all(&directory)?;
    let wrapper = directory.join("codex.exe");
    if !wrapper.is_file() {
        let staging = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        if let Err(error) = std::fs::copy(&executable, &staging) {
            let _ = std::fs::remove_file(&staging);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&staging, &wrapper) {
            let _ = std::fs::remove_file(&staging);
            if !wrapper.is_file() {
                return Err(error.into());
            }
        }
    }
    Ok(directory)
}

fn wrapper_scripts(windows: bool) -> Vec<(&'static str, &'static str)> {
    if windows {
        // Keep explicit codex.cmd calls available too. The default codex
        // command resolves to the native wrapper, preserving embedded quotes.
        vec![(
            "codex.cmd",
            "@echo off\r\n\"%ALERA_CODEX_LAUNCHER%\" agent-codex-launch -- %*\r\nexit /b %errorlevel%\r\n",
        )]
    } else {
        vec![(
            "codex",
            "#!/bin/sh\nexec \"$ALERA_CODEX_LAUNCHER\" agent-codex-launch -- \"$@\"\n",
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_terminal_wrapper_preserves_existing_wrapper_directories_and_user_home() {
        let runtime = tempfile::tempdir().unwrap();
        let existing = runtime.path().join("other-agent");
        let mut environment = BTreeMap::from([
            ("CODEX_HOME".into(), "selected-account".into()),
            (
                "ALERA_AGENT_WRAPPER_PATH".into(),
                existing.to_string_lossy().into_owned(),
            ),
        ]);
        prepare_codex_terminal_wrapper(runtime.path(), &mut environment).unwrap();
        let directories =
            std::env::split_paths(&environment["ALERA_AGENT_WRAPPER_PATH"]).collect::<Vec<_>>();
        assert_eq!(directories[1], existing);
        assert_eq!(environment["CODEX_HOME"], "selected-account");
        assert_eq!(
            environment["ALERA_CODEX_LAUNCHER"],
            std::env::current_exe().unwrap().to_string_lossy()
        );
        for (name, source) in wrapper_scripts(cfg!(windows)) {
            assert_eq!(
                std::fs::read_to_string(directories[0].join(name)).unwrap(),
                source
            );
        }
        #[cfg(windows)]
        assert_eq!(
            directories[0].join("codex.exe").metadata().unwrap().len(),
            std::env::current_exe().unwrap().metadata().unwrap().len()
        );
        let previous = environment["ALERA_AGENT_WRAPPER_PATH"].clone();
        prepare_codex_terminal_wrapper(runtime.path(), &mut environment).unwrap();
        assert_eq!(environment["ALERA_AGENT_WRAPPER_PATH"], previous);
    }

    #[test]
    fn codex_terminal_wrapper_forwards_arguments_on_windows_and_posix() {
        let windows = wrapper_scripts(true);
        assert!(windows[0].1.contains("-- %*"));
        assert!(!windows[0].1.contains("call "));
        assert_eq!(windows.len(), 1);
        assert!(wrapper_scripts(false)[0].1.contains("-- \"$@\""));
    }
}
