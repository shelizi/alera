use super::*;

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
