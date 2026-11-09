//! Safe user-initiated external application launch for existing workspace files.
//! Never invoke cmd /c, PowerShell, a shell string or write a Git Blob to disk.
use std::path::{Component, Path, PathBuf};

pub fn checked_workspace_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.components().next().is_none()
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("Unsafe relative path for external application".into());
    }
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    let target = dunce::canonicalize(root.join(relative))
        .map_err(|e| format!("File does not exist or cannot be accessed: {e}"))?;
    if !target.starts_with(&root) || !target.is_file() {
        return Err(
            "External application target is outside workspace or not a regular file".into(),
        );
    }
    Ok(target)
}

pub fn launch_external_file(root: &Path, relative: &Path) -> Result<(), String> {
    let target = checked_workspace_file(root, relative)?;
    if let Some(executable) =
        std::env::var_os("ALERA_EXTERNAL_EDITOR").filter(|value| !value.is_empty())
    {
        std::process::Command::new(executable)
            .arg(&target)
            .spawn()
            .map_err(|e| format!("Configured external editor failed to launch: {e}"))?;
        return Ok(());
    }

    // With no configured editor, use the OS-registered application for this
    // exact file path. FFI arguments are separate strings; no shell parsing.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }
        let wide = |text: &std::ffi::OsStr| text.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
        let open = wide(std::ffi::OsStr::new("open"));
        let path = wide(target.as_os_str());
        let result = unsafe {
            ShellExecuteW(
                0,
                open.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result <= 32 {
            return Err(format!("Windows has no registered application for this file (ShellExecute code {result}); configure ALERA_EXTERNAL_EDITOR"));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_existing_files_inside_workspace_are_launchable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/file a.txt"), "safe").unwrap();
        assert!(checked_workspace_file(dir.path(), Path::new("sub/file a.txt")).is_ok());
        for invalid in ["../escape.txt", "/absolute", ".", "sub", "missing.txt"] {
            assert!(
                checked_workspace_file(dir.path(), Path::new(invalid)).is_err(),
                "{invalid}"
            );
        }
    }
}
