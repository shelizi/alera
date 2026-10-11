# Codex Terminal Hooks

Alera starts managed Codex terminal profiles and direct `codex`, `codex.exe`, or `codex.cmd` startup commands with `--no-daemon`. This keeps lifecycle hooks in a process that inherits the terminal's Alera endpoint and session, workspace, and tab identifiers. A shared Codex daemon started outside that terminal can lack those identifiers, causing the hook script to exit without sending status events.

The flag is added when the terminal starts, including when an existing tab is restarted. It does not change the saved profile, approval policy, or sandbox settings. Existing `--no-daemon` arguments are preserved. Use a Codex version whose `codex --help` lists `--no-daemon`.

When Codex status hooks are enabled, new Alera terminal environments also prepend a terminal-local `codex` launcher to PATH. Manually running `codex` or `codex resume --last` therefore uses `--no-daemon` too. The launcher preserves an existing flag, the prompt `--` separator, the selected account, standard input/output, and the CLI exit code. It removes only its own wrapper directory before finding the installed Codex command, so it cannot recursively launch itself. The real Codex installation and user shell profiles are unchanged.

On Windows the default launcher is a native `codex.exe` entry point, so PowerShell preserves the prompt `--` separator and embedded quotes without script parameter binding or an extra batch parser. A cached copy of the sidecar provides this entry point, with a separate directory for each build so an update never overwrites an active launcher. Explicit `codex.exe` and `codex.cmd` commands also resolve through the terminal-local PATH.

Custom aliases, absolute executable paths, and scripts that replace PATH can bypass the terminal-local launcher. Add `--no-daemon` to those Codex invocations. Alera's direct startup-command normalization still handles `codex.exe` and `codex.cmd` when it constructs the startup command.

Existing Codex processes and shell environments keep their current execution mode. Finish their work, update the runtime host, and create or restart the terminal to use the corrected launcher. A GUI restart alone can reconnect to an older runtime host and does not update that host's terminal environments.
