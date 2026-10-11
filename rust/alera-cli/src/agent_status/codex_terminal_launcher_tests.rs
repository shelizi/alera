use super::*;
use crate::cli::{Cli, Command};
use clap::Parser;

#[test]
fn codex_terminal_launcher_preserves_resume_and_prompt_separator() {
    for (arguments, expected) in [
        (
            vec!["resume", "--last"],
            vec!["--no-daemon", "resume", "--last"],
        ),
        (
            vec!["--no-daemon", "resume", "--last"],
            vec!["--no-daemon", "resume", "--last"],
        ),
        (
            vec!["--", "--no-daemon"],
            vec!["--no-daemon", "--", "--no-daemon"],
        ),
        (
            vec!["--", "測試 \"quoted\" & $x"],
            vec!["--no-daemon", "--", "測試 \"quoted\" & $x"],
        ),
    ] {
        let arguments = arguments
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(isolated_arguments(&arguments), expected);
    }
}

#[test]
fn codex_terminal_launcher_cli_passes_codex_flags_through() {
    let cli = Cli::try_parse_from([
        "alera",
        "agent-codex-launch",
        "--",
        "--help",
        "resume",
        "--last",
        "--",
        "--no-daemon",
    ])
    .unwrap();
    let Command::AgentCodexLaunch { arguments } = cli.command else {
        panic!("expected Codex launcher");
    };
    assert_eq!(
        arguments,
        ["--help", "resume", "--last", "--", "--no-daemon"]
    );
}

#[test]
fn codex_terminal_launcher_removes_wrappers_before_resolving_the_real_command() {
    let runtime = tempfile::tempdir().unwrap();
    let wrapper = runtime.path().join("wrapper");
    let other_wrapper = runtime.path().join("other-wrapper");
    let real = runtime.path().join("real tool with spaces");
    std::fs::create_dir_all(&wrapper).unwrap();
    std::fs::create_dir_all(&real).unwrap();
    let name = if cfg!(windows) { "codex.cmd" } else { "codex" };
    std::fs::write(wrapper.join(name), "wrapper").unwrap();
    std::fs::write(real.join(name), "real command").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(real.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let environment = BTreeMap::from([
        (
            "PATH".into(),
            std::env::join_paths([&wrapper, &real, &other_wrapper])
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "ALERA_AGENT_WRAPPER_PATH".into(),
            std::env::join_paths([&wrapper, &other_wrapper])
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "ALERA_CODEX_WRAPPER_PATH".into(),
            wrapper.to_string_lossy().into_owned(),
        ),
    ]);
    let path = unwrapped_path(&environment);
    assert_eq!(
        std::env::split_paths(&path).collect::<Vec<_>>(),
        vec![real.clone(), other_wrapper]
    );
    assert_eq!(find_codex(&path), Some(real.join(name)));
}
