use alera_core::managed_agent_launch::ManagedAgentLaunch;

const NO_DAEMON: &str = "--no-daemon";

pub fn isolate_managed_codex_launch(agent: Option<&str>, launch: &mut ManagedAgentLaunch) {
    if agent != Some("codex") {
        return;
    }
    if !launch
        .arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| argument == NO_DAEMON)
    {
        // A shared Codex daemon does not inherit this terminal's hook routing
        // environment. Keep hook execution in the terminal-owned process.
        launch.arguments.insert(0, NO_DAEMON.to_string());
    }
}

pub fn isolate_codex_command(command: String) -> String {
    // Compound shell scripts own their invocation and quoting. Only rewrite
    // direct CLI commands, rather than attempting to parse arbitrary scripts.
    if command.contains([';', '&', '|', '\n', '\r']) {
        return command;
    }
    let trimmed = command.trim_start();
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let executable = &trimmed[..end];
    if !["codex", "codex.exe", "codex.cmd"]
        .iter()
        .any(|name| executable.eq_ignore_ascii_case(name))
    {
        return command;
    }
    let arguments = &trimmed[end..];
    if arguments
        .split_whitespace()
        .take_while(|argument| *argument != "--")
        .any(|argument| argument.trim_matches(['\'', '"']) == NO_DAEMON)
    {
        return command;
    }
    let offset = command.len() - trimmed.len() + end;
    let mut isolated = command;
    isolated.insert_str(offset, " --no-daemon");
    isolated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_host::orchestration::agent_startup_command::append_initial_prompt_argument;
    use crate::terminal_host::orchestration::managed_launch_shell_rendering::render_managed_launch;
    use alera_core::agent_descriptor::agent_descriptor;
    use alera_core::managed_agent_launch::build_managed_agent_launch;
    use serde_json::json;

    #[test]
    fn codex_terminal_launch_keeps_hooks_local_before_prompt_rendering_on_every_shell() {
        let mut launch = build_managed_agent_launch("codex", &json!({"webSearch": true})).unwrap();
        isolate_managed_codex_launch(Some("codex"), &mut launch);
        isolate_managed_codex_launch(Some("codex"), &mut launch);
        append_initial_prompt_argument(
            agent_descriptor("codex").unwrap(),
            &mut launch.arguments,
            "- Review the hook",
        );
        assert_eq!(
            launch.arguments,
            [NO_DAEMON, "--search", "--", "- Review the hook"]
        );
        for (shell, expected) in [
            (
                "/bin/zsh",
                "'codex' '--no-daemon' '--search' '--' '- Review the hook'",
            ),
            (
                "pwsh.exe",
                "'codex' '--no-daemon' '--search' '--' '- Review the hook'",
            ),
            (
                "cmd.exe",
                "\"codex\" \"--no-daemon\" \"--search\" \"--\" \"- Review the hook\"",
            ),
        ] {
            assert_eq!(render_managed_launch(&launch, shell), expected);
        }
    }

    #[test]
    fn codex_terminal_launch_preserves_command_arguments_and_resume() {
        for (command, expected) in [
            ("codex", "codex --no-daemon"),
            ("  codex --search", "  codex --no-daemon --search"),
            ("codex resume --last", "codex --no-daemon resume --last"),
            (
                "codex.cmd --model gpt-6.1-sol",
                "codex.cmd --no-daemon --model gpt-6.1-sol",
            ),
            ("codex.exe --search", "codex.exe --no-daemon --search"),
            (
                "codex --no-daemon resume --last",
                "codex --no-daemon resume --last",
            ),
            ("codex -- --no-daemon", "codex --no-daemon -- --no-daemon"),
        ] {
            assert_eq!(isolate_codex_command(command.to_string()), expected);
        }
    }

    #[test]
    fn codex_terminal_launch_preserves_other_agents_and_custom_wrappers() {
        for command in [
            "claude",
            "ccs codex",
            "my-codex-wrapper --search",
            "echo codex",
            "codex.exe --search && echo done",
            "codex --no-daemon; echo done",
        ] {
            assert_eq!(isolate_codex_command(command.to_string()), command);
        }
        let mut launch = build_managed_agent_launch("claude", &json!({})).unwrap();
        let original = launch.clone();
        isolate_managed_codex_launch(Some("claude"), &mut launch);
        assert_eq!(launch, original);
    }
}
