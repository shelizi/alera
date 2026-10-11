# Windows Agent Hook Relay

## Local Windows Validation

On 2026-10-11, `cargo +1.98.0 test --manifest-path rust/Cargo.toml --locked --target-dir D:\c\t -p alera-cli --bin alera agent_status:: -- --nocapture` passed 83 tests. The debug sidecar build also passed. Real batch-script invocations for Codex, Claude, Devin, Cursor, Grok, Antigravity, and Copilot produced identical parsed HTTP requests and stdout responses through a synthetic loopback receiver. Older-host and missing-executable fallbacks, plus invalid JSON handling, also passed.

Seven samples per path used the same 100 KB tool text plus Unicode JSON, with a fixed 40 ms receiver delay. Median elapsed time was 892.3 ms for PowerShell and 271.9 ms for the native relay. Median peak process-tree working set was 100.1 MiB and 28.0 MiB respectively, sampled every 3 ms. These are local debug-build measurements of transient hook processes, not a Release benchmark or a reduction in retained agent/MCP memory.

## JSON Allocation Follow-Up

The relay serializes a borrowed payload instead of constructing a second owned JSON tree, and releases the original stdin buffer before HTTP delivery. Parsing, number normalization, field order, missing optional fields, invalid-input handling, and retry behavior remain unchanged.

On 2026-10-11, the follow-up passed 84 agent-status tests and a debug sidecar build. The new regression compares the previous envelope's exact wire bytes for scalar, nested, Unicode, escaped, numeric, and duplicate-key inputs. Actual old/new relay invocations also produced identical HTTP body bytes for seven agent routes, and the shared batch-script checks passed for stdout responses, older-host and missing-executable fallbacks, and invalid JSON.

The before/after debug executables were warmed and then sampled in alternating order, seven samples per executable and payload, against a synthetic loopback receiver with an 80 ms response delay. Working set was sampled every 1 ms for the relay process alone, excluding the invoking shell and receiver. These measurements use a different boundary from the original process-tree comparison above.

| Payload | Input Bytes | Before Peak MiB | After Peak MiB | Before Median ms | After Median ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Long text | 100,038 | 12.92 | 12.77 | 156.36 | 119.63 |
| Long text | 900,038 | 15.65 | 14.11 | 139.41 | 142.44 |
| 10,000 nested objects | 877,791 | 31.84 | 23.34 | 196.01 | 196.94 |

The nested payload reduced median peak working set by 8.50 MiB, about 27%. Timing differences for the larger payloads were within run variation: before/after elapsed-time MAD was 1.87/7.18 ms for long text and 3.13/16.66 ms for nested objects. These results demonstrate transient allocation savings; they do not establish a retained agent-memory reduction or a timing improvement.

## Behavior

The runtime's shared Windows lifecycle hook script can send its HTTP event through the bundled sidecar's hidden `agent-hook-relay` command. This replaces a PowerShell process for each managed batch-hook invocation without changing agent executables, package launchers, MCP configuration, terminal shells, or account settings.

New Windows terminal environments carry `ALERA_AGENT_HOOK_RELAY`, pointing to the sidecar that prepared them. The relay runs synchronously without creating a Tokio runtime, reads UTF-8 JSON from stdin, and sends the existing identity, event, version, and payload fields to the loopback hook receiver. It remains silent and exits successfully when routing is absent, JSON is invalid, or the host cannot be reached. Completion events retain three attempts with 50 ms and 100 ms backoff; other events receive one attempt. Each HTTP attempt has a one-second deadline and a 250 ms connection timeout.

The batch script still owns Antigravity/Copilot stdout responses, the Claude/Grok compatibility guard, and endpoint-file loading before forwarding. Environments from older hosts, or terminals whose relay executable is unavailable, use the existing PowerShell path. Existing processes are not restarted. The optimization requires the new sidecar and a new terminal environment; it does not reduce the agent or MCP server's own retained memory. POSIX hooks and agents with direct HTTP/plugin integrations keep their existing transport.
