# Windows Agent Hook Relay

## Local Windows Validation

On 2026-10-11, `cargo +1.98.0 test --manifest-path rust/Cargo.toml --locked --target-dir D:\c\t -p alera-cli --bin alera agent_status:: -- --nocapture` passed 83 tests. The debug sidecar build also passed. Real batch-script invocations for Codex, Claude, Devin, Cursor, Grok, Antigravity, and Copilot produced identical parsed HTTP requests and stdout responses through a synthetic loopback receiver. Older-host and missing-executable fallbacks, plus invalid JSON handling, also passed.

Seven samples per path used the same 100 KB tool text plus Unicode JSON, with a fixed 40 ms receiver delay. Median elapsed time was 892.3 ms for PowerShell and 271.9 ms for the native relay. Median peak process-tree working set was 100.1 MiB and 28.0 MiB respectively, sampled every 3 ms. These are local debug-build measurements of transient hook processes, not a Release benchmark or a reduction in retained agent/MCP memory.

## Behavior

The runtime's shared Windows lifecycle hook script can send its HTTP event through the bundled sidecar's hidden `agent-hook-relay` command. This replaces a PowerShell process for each managed batch-hook invocation without changing agent executables, package launchers, MCP configuration, terminal shells, or account settings.

New Windows terminal environments carry `ALERA_AGENT_HOOK_RELAY`, pointing to the sidecar that prepared them. The relay runs synchronously without creating a Tokio runtime, reads UTF-8 JSON from stdin, and sends the existing identity, event, version, and payload fields to the loopback hook receiver. It remains silent and exits successfully when routing is absent, JSON is invalid, or the host cannot be reached. Completion events retain three attempts with 50 ms and 100 ms backoff; other events receive one attempt. Each HTTP attempt has a one-second deadline and a 250 ms connection timeout.

The batch script still owns Antigravity/Copilot stdout responses, the Claude/Grok compatibility guard, and endpoint-file loading before forwarding. Environments from older hosts, or terminals whose relay executable is unavailable, use the existing PowerShell path. Existing processes are not restarted. The optimization requires the new sidecar and a new terminal environment; it does not reduce the agent or MCP server's own retained memory. POSIX hooks and agents with direct HTTP/plugin integrations keep their existing transport.
