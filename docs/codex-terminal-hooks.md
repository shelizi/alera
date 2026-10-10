# Codex Terminal Hooks

Alera starts managed Codex terminal profiles and direct `codex`, `codex.exe`, or `codex.cmd` startup commands with `--no-daemon`. This keeps lifecycle hooks in a process that inherits the terminal's Alera endpoint and session, workspace, and tab identifiers. A shared Codex daemon started outside that terminal can lack those identifiers, causing the hook script to exit without sending status events.

The flag is added when the terminal starts, including when an existing tab is restarted. It does not change the saved profile, approval policy, or sandbox settings. Existing `--no-daemon` arguments are preserved. Use a Codex version whose `codex --help` lists `--no-daemon`.

Custom wrappers and compound shell scripts keep their original command. Add `--no-daemon` to the Codex invocation inside the wrapper or script. Manually starting Codex inside a shell also requires `codex --no-daemon`, or `codex --no-daemon resume --last` to resume a conversation.

Existing Codex processes keep their current execution mode. Finish their work and relaunch them to use the corrected startup command.
