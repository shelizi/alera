use super::*;

#[test]
fn agy_hooks_execute_through_go_style_cmd_arguments() {
    use std::os::windows::process::CommandExt;

    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("user space (dev)'s & 中文");
    std::fs::create_dir(&directory).unwrap();
    let script = directory.join("alera-runtime-agent-hook.cmd");
    std::fs::write(
        &script,
        "@echo off\r\necho %ALERA_AGENT_TYPE%:%ALERA_AGENT_HOOK_EVENT%\r\n",
    )
    .unwrap();
    for event in ["PreInvocation", "PostInvocation", "PostToolUse", "Stop"] {
        let command = managed_command(&script, "agy", event);
        assert!(is_alera_managed_definition(&json!({"command": command})));
        // Go's exec.Command quotes the /c argument using Windows argv rules,
        // unlike Rust's cmd-specific escaping and the batch-file probe below.
        let result = alera_core::child_process::windowless_command("cmd.exe")
            .raw_arg(format!("/c \"{}\"", command.replace('"', "\\\"")))
            .output()
            .unwrap();
        assert!(result.status.success(), "{event}: {:?}", result);
        assert!(result.stderr.is_empty(), "{event}: {:?}", result);
        assert_eq!(
            String::from_utf8(result.stdout).unwrap().trim(),
            format!("agy:{event}")
        );
        let mut config = Map::new();
        apply_agy_bundle(&mut config, &script);
        apply_agy_bundle(&mut config, &script);
        assert_eq!(agy_bundle(&config)[event].as_array().unwrap().len(), 1);
    }
}

#[test]
fn native_hooks_execute_batch_paths_with_spaces_and_parentheses() {
    let root = tempfile::tempdir().expect("hook test directory");
    let script_dir = root.path().join("user space (dev)'s");
    std::fs::create_dir(&script_dir).expect("spaced script directory");
    let script = script_dir.join("alera-runtime-agent-hook.cmd");
    std::fs::write(
        &script,
        "@echo off\r\necho %ALERA_AGENT_TYPE%:%ALERA_AGENT_HOOK_EVENT%\r\nexit /b 0\r\n",
    )
    .expect("test hook script");
    let probe = root.path().join("probe.cmd");
    for agent in ["codex", "grok", "agy"] {
        for event in ["UserPromptSubmit", "PreToolUse", "Stop"] {
            let command = managed_command(&script, agent, event);
            std::fs::write(&probe, format!("@echo off\r\n{command}\r\n"))
                .expect("hook command probe");
            let result = alera_core::child_process::windowless_command("cmd.exe")
                .args(["/d", "/c"])
                .arg(&probe)
                .output()
                .expect("execute hook probe");
            assert!(
                result.status.success(),
                "{agent}/{event}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&result.stdout).trim(),
                format!("{agent}:{event}")
            );
        }
    }
}
