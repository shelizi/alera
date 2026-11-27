//! The command line and shell invocation Alera hands to the platform shell.
//!
//! On Windows, Win32 `CreateProcess` only resolves `.exe` files and cannot
//! launch `.cmd`/`.bat` scripts or shims (such as those installed by Scoop,
//! npm, pnpm, and nvm) without invoking `cmd.exe`. Quoting rules mirror Dart's
//! `_getShellArguments` so calls across Rust and Flutter remain consistent.

/// How a command reaches its shell. Windows needs a raw command line because
/// `cmd.exe` does not read the backslash escaping `std::process::Command`
/// applies to ordinary arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellInvocation {
    Posix {
        program: String,
        arguments: Vec<String>,
    },
    Windows {
        program: String,
        raw_arguments: String,
    },
}

pub fn shell_invocation(executable: &str, arguments: &[String]) -> ShellInvocation {
    if cfg!(windows) {
        windows_shell_invocation(executable, arguments)
    } else {
        posix_shell_invocation(executable, arguments)
    }
}

pub fn posix_shell_invocation(executable: &str, arguments: &[String]) -> ShellInvocation {
    let mut line = single_quoted(executable);
    for argument in arguments {
        line.push(' ');
        line.push_str(&single_quoted(argument));
    }
    ShellInvocation::Posix {
        program: "/bin/sh".to_string(),
        arguments: vec!["-c".to_string(), line],
    }
}

/// `/d` skips AutoRun scripts, and `/s` makes `cmd.exe` strip the outer quotes
/// of the `/c` argument and take everything between them verbatim. That is what
/// lets each token carry its own quotes without further escaping.
pub fn windows_shell_invocation(executable: &str, arguments: &[String]) -> ShellInvocation {
    let mut line = String::from("/d /s /c \"");
    line.push_str(&double_quoted(executable));
    for argument in arguments {
        line.push(' ');
        line.push_str(&double_quoted(argument));
    }
    line.push('"');
    ShellInvocation::Windows {
        program: "cmd.exe".to_string(),
        raw_arguments: line,
    }
}

/// Resolves an executable on Windows by checking `PATHEXT` (`.COM;.EXE;.BAT;.CMD`)
/// against the working directory, PATH, and standard user tool directories.
#[cfg(windows)]
pub fn resolve_windows_executable(
    executable: &str,
    working_directory: Option<&str>,
    environment_path: Option<&str>,
) -> String {
    if executable.contains(['/', '\\']) {
        return executable.to_string();
    }
    let extensions = if std::path::Path::new(executable).extension().is_some() {
        vec![String::new()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(str::to_string)
            .collect()
    };
    let mut directories = Vec::new();
    if let Some(working_directory) = working_directory {
        directories.push(std::path::PathBuf::from(working_directory));
    } else if let Ok(current_directory) = std::env::current_dir() {
        directories.push(current_directory);
    }
    if let Some(path) = environment_path {
        directories.extend(std::env::split_paths(path));
    } else if let Ok(path) = std::env::var("PATH") {
        directories.extend(std::env::split_paths(&path));
    }
    // Fallback directories for user-level package managers (Scoop, npm, pnpm, Cargo)
    // to handle tools installed mid-session before PATH propagation reaches the process.
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let user_path = std::path::Path::new(&user_profile);
        directories.push(user_path.join("scoop").join("shims"));
        directories.push(user_path.join(".cargo").join("bin"));
        directories.push(user_path.join(".local").join("bin"));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        directories.push(std::path::Path::new(&app_data).join("npm"));
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        directories.push(std::path::Path::new(&local_app_data).join("pnpm"));
    }

    for directory in directories {
        for extension in &extensions {
            let candidate = directory.join(format!("{executable}{extension}"));
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    executable.to_string()
}

// A resolved native executable needs no cmd.exe wrapper for a literal version
// query. A hidden shell does not guarantee that a console grandchild remains
// hidden on Windows, and cmd.exe also expands percent-delimited path tokens.
// Keep the shell for .cmd/.bat shims, unresolved commands and general calls.
#[cfg(windows)]
fn direct_native_version_probe(executable: &str, arguments: &[String]) -> bool {
    let path = std::path::Path::new(executable);
    arguments.len() == 1
        && arguments[0] == "--version"
        && path.is_absolute()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

/// Builds a windowless asynchronous command, using the shell when needed.
#[cfg(feature = "async-process")]
pub fn windowless_async_shell_command(
    executable: &str,
    arguments: &[String],
    working_directory: Option<&str>,
    environment_path: Option<&str>,
) -> tokio::process::Command {
    #[cfg(windows)]
    let resolved = resolve_windows_executable(executable, working_directory, environment_path);
    #[cfg(not(windows))]
    let resolved = {
        let _ = (working_directory, environment_path);
        executable.to_string()
    };

    #[cfg(windows)]
    if direct_native_version_probe(&resolved, arguments) {
        let mut command = crate::child_process::windowless_async_command(&resolved);
        command.args(arguments);
        return command;
    }

    match shell_invocation(&resolved, arguments) {
        ShellInvocation::Posix { program, arguments } => {
            let mut command = crate::child_process::windowless_async_command(program);
            command.args(arguments);
            command
        }
        ShellInvocation::Windows {
            program,
            raw_arguments,
        } => {
            #[cfg_attr(not(windows), allow(unused_mut))]
            let mut command = crate::child_process::windowless_async_command(program);
            #[cfg(windows)]
            {
                command.raw_arg(&raw_arguments);
            }
            #[cfg(not(windows))]
            {
                let _ = raw_arguments;
            }
            command
        }
    }
}

/// Builds a windowless synchronous command, using the shell when needed.
pub fn windowless_shell_command(
    executable: &str,
    arguments: &[String],
    working_directory: Option<&str>,
    environment_path: Option<&str>,
) -> std::process::Command {
    #[cfg(windows)]
    let resolved = resolve_windows_executable(executable, working_directory, environment_path);
    #[cfg(not(windows))]
    let resolved = {
        let _ = (working_directory, environment_path);
        executable.to_string()
    };

    #[cfg(windows)]
    if direct_native_version_probe(&resolved, arguments) {
        let mut command = crate::child_process::windowless_command(&resolved);
        command.args(arguments);
        return command;
    }

    match shell_invocation(&resolved, arguments) {
        ShellInvocation::Posix { program, arguments } => {
            let mut command = crate::child_process::windowless_command(program);
            command.args(arguments);
            command
        }
        ShellInvocation::Windows {
            program,
            raw_arguments,
        } => {
            #[cfg_attr(not(windows), allow(unused_mut))]
            let mut command = crate::child_process::windowless_command(program);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.raw_arg(&raw_arguments);
            }
            #[cfg(not(windows))]
            {
                let _ = raw_arguments;
            }
            command
        }
    }
}

fn single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn double_quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(all(test, windows))]
mod windows_version_probe_tests {
    use super::*;

    #[test]
    fn native_version_probe_preserves_exact_path_and_bypasses_cmd() {
        let executable = r"C:\Program Files\%ALERA_PROBE%\agy.EXE";
        let arguments = vec!["--version".to_owned()];
        let command = windowless_shell_command(executable, &arguments, None, None);
        assert_eq!(command.get_program(), std::ffi::OsStr::new(executable));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("--version")]
        );
    }

    #[test]
    fn version_probe_keeps_batch_wrappers_and_other_calls_in_shell() {
        let version = vec!["--version".to_owned()];
        for executable in [r"C:\Agents\agy.cmd", r"C:\Agents\agy.BAT"] {
            let command = windowless_shell_command(executable, &version, None, None);
            assert_eq!(command.get_program(), std::ffi::OsStr::new("cmd.exe"));
        }
        for arguments in [vec![], vec!["--version".into(), "--help".into()]] {
            let command = windowless_shell_command(r"C:\Agents\agy.exe", &arguments, None, None);
            assert_eq!(command.get_program(), std::ffi::OsStr::new("cmd.exe"));
        }
        assert!(!direct_native_version_probe("agy.exe", &version));
    }

    #[cfg(feature = "async-process")]
    #[test]
    fn async_version_probe_uses_the_same_direct_native_path() {
        let executable = r"C:\Program Files\%ALERA_PROBE%\agy.EXE";
        let arguments = vec!["--version".to_owned()];
        let command = windowless_async_shell_command(executable, &arguments, None, None);
        assert_eq!(
            command.as_std().get_program(),
            std::ffi::OsStr::new(executable)
        );
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("--version")]
        );
        let shim = windowless_async_shell_command(r"C:\Agents\agy.cmd", &arguments, None, None);
        assert_eq!(shim.as_std().get_program(), std::ffi::OsStr::new("cmd.exe"));
    }
}
