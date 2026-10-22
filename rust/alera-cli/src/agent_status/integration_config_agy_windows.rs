use std::path::Path;

use base64::Engine as _;

pub(super) fn managed_command(script: &Path, event: &str) -> String {
    // AGY's Go runner escapes embedded double quotes as \" for cmd /c, which
    // cmd treats literally. Keep the command free of quotes and shell metacharacters
    // from profile paths. The script tag keeps managed-hook cleanup interoperable.
    let source = format!(
        "$env:ALERA_AGENT_TYPE='agy'; $env:ALERA_AGENT_HOOK_EVENT='{}'; & '{}'",
        event.replace('\'', "''"),
        script.display().to_string().replace('\'', "''"),
    );
    let bytes: Vec<u8> = source.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(
        "set ALERA_MANAGED_HOOK=agent-hooks/alera-runtime-agent-hook.cmd&& powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"
    )
}
