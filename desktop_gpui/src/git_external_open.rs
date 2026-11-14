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

/// The MCP Host only permits explicit Zed or VS Code selection. Prepare a
/// separate argument vector; never accept an executable from MCP arguments,
/// use a shell string, or fall back to an unrelated installed editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpExternalEditorLaunch {
    pub editor: &'static str,
    pub target: PathBuf,
    pub leading_arguments: &'static [&'static str],
    pub file_argument: String,
}

pub fn prepare_mcp_external_editor_launch(
    root: &Path,
    relative: &Path,
    editor: &str,
    line: Option<u64>,
    column: Option<u64>,
) -> Result<McpExternalEditorLaunch, String> {
    let (editor, leading_arguments) = match editor {
        "zed" => ("zed", &[][..]),
        "vscode" => ("vscode", &["--goto"][..]),
        _ => return Err("Unsupported MCP external editor".into()),
    };
    if line.is_some_and(|value| !(1..=1_000_000_000).contains(&value))
        || column.is_some_and(|value| !(1..=1_000_000_000).contains(&value))
        || (column.is_some() && line.is_none())
    {
        return Err("External editor position must be a valid 1-based line/column".into());
    }
    let target = checked_workspace_file(root, relative)?;
    let mut file_argument = target
        .to_str()
        .ok_or("External editor cannot open a non-Unicode path")?
        .to_owned();
    if let Some(line) = line {
        file_argument.push_str(&format!(":{line}"));
        if let Some(column) = column {
            file_argument.push_str(&format!(":{column}"));
        }
    }
    Ok(McpExternalEditorLaunch {
        editor,
        target,
        leading_arguments,
        file_argument,
    })
}

fn mcp_external_editor_candidates(editor: &str) -> Vec<PathBuf> {
    let mut candidates = match editor {
        "zed" => vec![PathBuf::from(if cfg!(target_os = "windows") {
            "zed.exe"
        } else {
            "zed"
        })],
        // A Windows PATH "code" command can resolve to a .cmd script. Prefer
        // a direct .exe and use guarded .cmd fallbacks only below.
        "vscode" => vec![PathBuf::from(if cfg!(target_os = "windows") {
            "Code.exe"
        } else {
            "code"
        })],
        _ => Vec::new(),
    };
    #[cfg(target_os = "windows")]
    if editor == "vscode" {
        if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(&dir).join("Programs/Microsoft VS Code/Code.exe"));
            candidates.push(PathBuf::from(dir).join("Programs/Microsoft VS Code/bin/code.cmd"));
        }
        if let Some(dir) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(dir).join("Microsoft VS Code/Code.exe"));
        }
        if let Some(dir) = std::env::var_os("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(dir).join("Microsoft VS Code/Code.exe"));
        }
        // Classic's command registry also supports code.cmd on Windows.
        candidates.push(PathBuf::from("code.cmd"));
    }
    #[cfg(target_os = "macos")]
    {
        if editor == "zed" {
            candidates.push(PathBuf::from("/Applications/Zed.app/Contents/MacOS/cli"));
        } else if editor == "vscode" {
            candidates.push(PathBuf::from(
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            ));
        }
    }
    candidates
}

/// Resolve a fixed executable name only from absolute PATH directories, never
/// from the caller's working tree or an empty/relative PATH segment. Windows
/// CreateProcess otherwise may select an attacker-controlled local Code.exe.
fn resolve_editor_candidate_from_paths(
    candidate: &Path,
    directories: &[PathBuf],
) -> Option<PathBuf> {
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    if candidate.components().count() != 1 {
        return None;
    }
    directories
        .iter()
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(candidate))
        .find(|file| file.is_file())
}

fn resolve_editor_candidate(candidate: &Path) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")
        .map(|raw| std::env::split_paths(&raw).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_editor_candidate_from_paths(candidate, &paths)
}

/// A successful spawn is the earliest true external-editor acknowledgment.
/// Native does not invent an Alera tab or silently switch to OS-default apps.
pub fn launch_mcp_external_editor(
    root: &Path,
    relative: &Path,
    editor: &str,
    line: Option<u64>,
    column: Option<u64>,
) -> Result<(), String> {
    let spec = prepare_mcp_external_editor_launch(root, relative, editor, line, column)?;
    for candidate in mcp_external_editor_candidates(spec.editor) {
        let Some(executable) = resolve_editor_candidate(&candidate) else {
            continue;
        };
        // Reject Windows shell-metacharacter filenames before attempting a
        // batch launcher. Native executables are preferred and unrestricted.
        #[cfg(target_os = "windows")]
        if executable.extension().and_then(|ext| ext.to_str()) == Some("cmd")
            && spec.file_argument.chars().any(|ch| {
                matches!(
                    ch,
                    '&' | '|' | '<' | '>' | '^' | '%' | '!' | '"' | '\r' | '\n'
                )
            })
        {
            continue;
        }
        let mut command = std::process::Command::new(&executable);
        command
            .args(spec.leading_arguments)
            .arg(&spec.file_argument)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "External editor {} could not be launched: {error}",
                    spec.editor
                ))
            }
        }
    }
    Err(format!(
        "Requested external editor {} is not installed or on PATH",
        spec.editor
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_external_editor_executable_ignores_relative_search_paths() {
        let folder = tempfile::tempdir().unwrap();
        let exe = folder.path().join("zed.exe");
        std::fs::write(&exe, b"fake executable bytes; never launched").unwrap();
        let untrusted = PathBuf::from(".");
        assert_eq!(
            resolve_editor_candidate_from_paths(Path::new("zed.exe"), &[untrusted]),
            None,
        );
        assert_eq!(
            resolve_editor_candidate_from_paths(Path::new("zed.exe"), &[folder.path().to_owned()]),
            Some(exe),
        );
        assert_eq!(
            resolve_editor_candidate_from_paths(
                Path::new("../zed.exe"),
                &[folder.path().to_owned()]
            ),
            None,
        );
    }

    #[test]
    fn mcp_external_editor_file_position_uses_classic_arguments_without_shell() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/file name.rs"), "fn main() {}").unwrap();
        let relative = Path::new("src/file name.rs");
        let zed =
            prepare_mcp_external_editor_launch(dir.path(), relative, "zed", Some(17), Some(5))
                .unwrap();
        assert_eq!(zed.editor, "zed");
        assert!(zed.leading_arguments.is_empty());
        assert!(zed.file_argument.ends_with("file name.rs:17:5"));
        let vscode =
            prepare_mcp_external_editor_launch(dir.path(), relative, "vscode", Some(23), None)
                .unwrap();
        assert_eq!(vscode.leading_arguments, ["--goto"]);
        assert!(vscode.file_argument.ends_with("file name.rs:23"));
        assert!(
            prepare_mcp_external_editor_launch(dir.path(), relative, "zed", None, None,)
                .unwrap()
                .file_argument
                .ends_with("file name.rs")
        );
        for editor in ["alera", "unknown", "code.cmd"] {
            assert!(
                prepare_mcp_external_editor_launch(dir.path(), relative, editor, None, None,)
                    .is_err()
            );
        }
        for (line, column) in [
            (None, Some(1)),
            (Some(0), None),
            (Some(1_000_000_001), None),
            (Some(9), Some(0)),
        ] {
            assert!(
                prepare_mcp_external_editor_launch(dir.path(), relative, "zed", line, column,)
                    .is_err()
            );
        }
        assert!(prepare_mcp_external_editor_launch(
            dir.path(),
            Path::new("../outside.rs"),
            "vscode",
            None,
            None,
        )
        .is_err());
    }

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
