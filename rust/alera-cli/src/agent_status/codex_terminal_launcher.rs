use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alera_core::child_process::windowless_command;

pub fn codex_terminal_wrapper_arguments() -> Option<Vec<String>> {
    std::env::var_os("ALERA_CODEX_WRAPPER_PATH")?;
    let mut arguments = std::env::args();
    let executable = arguments.next()?;
    if !Path::new(&executable)
        .file_stem()?
        .eq_ignore_ascii_case("codex")
    {
        return None;
    }
    Some(arguments.collect())
}

pub fn run_codex_terminal_launcher(arguments: &[String]) -> i32 {
    // The wrapper runs inside the user's shell, whose rc files have already
    // resolved PATH and account variables. Do not replace that environment
    // with the host's cached login-shell environment.
    let mut environment = std::env::vars().collect::<BTreeMap<_, _>>();
    // Windows environment names are case insensitive, unlike BTreeMap keys.
    environment.insert("PATH".into(), std::env::var("PATH").unwrap_or_default());
    let path = unwrapped_path(&environment);
    let Some(executable) = find_codex(&path) else {
        eprintln!("Codex was not found on this terminal's PATH.");
        return 127;
    };
    let arguments = isolated_arguments(arguments);
    let mut command = if cfg!(windows)
        && executable.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        }) {
        alera_core::process_shell::windowless_shell_command(
            &executable.to_string_lossy(),
            &arguments,
            None,
            Some(&path),
        )
    } else {
        let mut command = windowless_command(&executable);
        command.args(&arguments);
        command
    };
    command.env("PATH", path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A TUI must inherit the existing ConPTY console. Only clear the
        // windowless flag when a console is already attached, so GUI callers
        // still cannot create a visible console window.
        if unsafe { windows::Win32::System::Console::GetConsoleCP() } != 0 {
            command.creation_flags(0);
            if let Err(error) = install_console_control_handler() {
                eprintln!("Failed to prepare Codex console control handling: {error}");
                return 1;
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        eprintln!("Failed to start Codex: {error}");
        1
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("Failed to start Codex: {error}");
            1
        }
    }
}

fn isolated_arguments(arguments: &[String]) -> Vec<String> {
    let mut result = arguments.to_vec();
    if !arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| argument == "--no-daemon")
    {
        result.insert(0, "--no-daemon".into());
    }
    result
}

fn unwrapped_path(environment: &BTreeMap<String, String>) -> String {
    let wrappers = environment
        .get("ALERA_CODEX_WRAPPER_PATH")
        .map(|path| std::env::split_paths(path).collect::<Vec<_>>())
        .unwrap_or_default();
    let directories = environment
        .get("PATH")
        .map(|path| {
            std::env::split_paths(path)
                .filter(|entry| !wrappers.iter().any(|wrapper| same_path(entry, wrapper)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    std::env::join_paths(directories)
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn install_console_control_handler() -> windows::core::Result<()> {
    use windows::Win32::System::Console::SetConsoleCtrlHandler;
    // Protect only this waiting launcher. The real CLI must still receive
    // Ctrl+C normally rather than inherit the null handler's ignore flag.
    unsafe { SetConsoleCtrlHandler(None, false)? };
    unsafe { SetConsoleCtrlHandler(Some(handle_console_control), true) }
}

#[cfg(windows)]
unsafe extern "system" fn handle_console_control(control: u32) -> windows::core::BOOL {
    use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    windows::core::BOOL::from(matches!(control, CTRL_C_EVENT | CTRL_BREAK_EVENT))
}

fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn find_codex(path: &str) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.com", "codex.cmd", "codex.bat"]
    } else {
        &["codex"]
    };
    for directory in std::env::split_paths(path) {
        for name in names {
            let candidate = directory.join(name);
            if !candidate.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if candidate.metadata().ok()?.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
#[path = "codex_terminal_launcher_tests.rs"]
mod tests;
